//! Original UpdatePortalInterconnection/ParsePortal/ParsePortalsInSector source
//! reconstruction (PC RVAs 0x450a0/0x44600/0x44da0). Same scene-owned portals,
//! sectors, loaded geometry and camera as scene_visibility; no alternate scene.
use crate::frame_portal::PortalKey;
use crate::frame_sector::{math, SectorAllocationKind, SectorError};
use crate::model_loading::ModelArena;
use crate::scene_ownership::{SceneKey, SectorKey};
use crate::scene_visibility::{
    append, distance_yzx, distance_zyx, number, vector_sub, CameraView, Result, SceneVisibility,
    SceneVisibilityError, SceneVisibilityOptions, CULLED, IN_WALK, ON, VISIBLE,
};

pub(crate) fn portal_vector(arena: &ModelArena, key: PortalKey, at: usize) -> Result<[f32; 3]> {
    let p = arena.portal(key)?;
    let mut v = [0.; 3];
    for (i, out) in v.iter_mut().enumerate() {
        *out = number(f32::from_bits(
            p.known_word(at + i * 4)
                .ok_or(SceneVisibilityError::InvalidRing)?,
        ))?;
    }
    Ok(v)
}
fn portal_scalar(arena: &ModelArena, key: PortalKey, at: usize) -> Result<f32> {
    number(f32::from_bits(
        arena
            .portal(key)?
            .known_word(at)
            .ok_or(SceneVisibilityError::InvalidRing)?,
    ))
}
fn square(v: [f32; 3]) -> Result<f32> {
    // Raw 44e0c/44e95: Y/Z products are combined before X, rather than the
    // source-level Square spelling's X/Y/Z association.
    Ok(math::squared(v)?)
}
pub(crate) fn portal_vertices(arena: &ModelArena, key: PortalKey) -> Result<Vec<[f32; 3]>> {
    let count = arena.portal(key)?.vertex_count();
    if !(3..=8).contains(&count) {
        return Err(SceneVisibilityError::Unsupported(
            "portal fixed vertex array",
        ));
    }
    let mut out = Vec::new();
    for i in 0..count as usize {
        append(&mut out, portal_vector(arena, key, 0x84 + i * 12)?)?;
    }
    Ok(out)
}
fn portal_distance(arena: &ModelArena, key: PortalKey, position: [f32; 3]) -> Result<f32> {
    let n = portal_vector(arena, key, 0xe4)?;
    distance_yzx(
        [n[0], n[1], n[2], portal_scalar(arena, key, 0xf0)?],
        position,
    )
}
impl ModelArena {
    pub(crate) fn visibility_sector_ring(&self, scene: SceneKey) -> Result<Vec<SectorKey>> {
        let s = self.scene(scene)?;
        let mut key = s.sector_head;
        let mut out = Vec::new();
        for _ in 0..s.sector_count {
            let k = key.ok_or(SceneVisibilityError::InvalidRing)?;
            if out.contains(&k) || self.sector(k)?.scene != Some(scene) {
                return Err(SceneVisibilityError::InvalidRing);
            }
            append(&mut out, k)?;
            key = self.sector(k)?.geometry.next;
        }
        if key != s.sector_head {
            return Err(SceneVisibilityError::InvalidRing);
        }
        Ok(out)
    }
    pub(crate) fn visibility_portal_ring(&self, scene: SceneKey) -> Result<Vec<PortalKey>> {
        let s = self.scene(scene)?;
        let mut key = s.portal_head;
        let mut out = Vec::new();
        for _ in 0..s.portal_count {
            let k = key.ok_or(SceneVisibilityError::InvalidRing)?;
            if out.contains(&k) || self.portal(k)?.scene != Some(scene) {
                return Err(SceneVisibilityError::InvalidRing);
            }
            append(&mut out, k)?;
            key = self.portal(k)?.next;
        }
        if key != s.portal_head {
            return Err(SceneVisibilityError::InvalidRing);
        }
        Ok(out)
    }
    fn visibility_incoming_push(&mut self, sector: SectorKey, p: PortalKey) -> Result<()> {
        let i = self.sector_index(sector)?;
        let g = &self.sectors[i].geometry;
        if g.update_interrupted {
            return Err(SectorError::Interrupted.into());
        }
        if g.incoming_portals.len() >= g.incoming_capacity {
            let capacity = g
                .incoming_capacity
                .checked_add(32)
                .ok_or(SectorError::Extent)?;
            self.sectors[i].geometry.incoming_capacity = capacity;
            let new = self.sector_storage.allocate(
                SectorAllocationKind::IncomingPortals,
                capacity.checked_mul(4).ok_or(SectorError::Extent)?,
            )?;
            let Some(new) = new else {
                self.sectors[i].geometry.update_interrupted = true;
                return Err(SectorError::Allocation.into());
            };
            self.sector_storage
                .free(self.sectors[i].geometry.incoming_buffer)?;
            self.sectors[i].geometry.incoming_buffer = Some(new);
        }
        append(&mut self.sectors[i].geometry.incoming_portals, p)
    }
    /// Native connection stores precede incoming-list appends. A modeled native
    /// allocation-null failure retains that prefix and blocks stale-success reuse.
    pub fn update_portal_interconnection(&mut self, scene: SceneKey) -> Result<()> {
        let portals = self.visibility_portal_ring(scene)?;
        let sectors = self.visibility_sector_ring(scene)?;
        for sector in sectors
            .iter()
            .copied()
            .chain([self.scene(scene)?.sectors[0]])
        {
            if self.sector(sector)?.geometry.update_interrupted {
                return Err(SectorError::Interrupted.into());
            }
        }
        for (at, &p) in portals.iter().enumerate() {
            if self.portal(p)?.connected_sector.is_none() {
                for &other in &portals[at + 1..] {
                    if self.portal(other)?.connected_sector.is_some() {
                        continue;
                    }
                    let a = portal_vertices(self, p)?;
                    let b = portal_vertices(self, other)?;
                    let mut lo = a[0];
                    let mut hi = a[0];
                    let mut ca = [0.; 3];
                    let mut cb = [0.; 3];
                    for (vertices, center) in [(&a, &mut ca), (&b, &mut cb)] {
                        for v in vertices {
                            for axis in 0..3 {
                                if lo[axis] > v[axis] {
                                    lo[axis] = v[axis];
                                }
                                if hi[axis] < v[axis] {
                                    hi[axis] = v[axis];
                                }
                                center[axis] = math::add(center[axis], v[axis])?;
                            }
                        }
                        let reciprocal =
                            number(crate::native_precision::div(1., vertices.len() as f32))?;
                        for c in center {
                            *c = math::mul(*c, reciprocal)?;
                        }
                    }
                    let mut matched = true;
                    for axis in 0..3 {
                        if f64::from(math::sub(hi[axis], lo[axis])?) < 0.01 {
                            hi[axis] =
                                crate::scene_visibility::arithmetic::add_constant(lo[axis], 0.1)?;
                        }
                        if math::sub(cb[axis], ca[axis])?.abs()
                            > crate::scene_visibility::arithmetic::mul_constant(
                                math::sub(hi[axis], lo[axis])?,
                                0.1,
                            )?
                        {
                            matched = false;
                        }
                    }
                    if !matched {
                        continue;
                    }
                    let a = self
                        .portal(p)?
                        .sector
                        .ok_or(SceneVisibilityError::InvalidRing)?;
                    let b = self
                        .portal(other)?
                        .sector
                        .ok_or(SceneVisibilityError::InvalidRing)?;
                    let pi = self.portal_index(p)?;
                    let oi = self.portal_index(other)?;
                    if a == b {
                        for i in [pi, oi] {
                            let flags = self.portal_frames[i].flags() | 0x80;
                            self.portal_frames[i].put(8, flags);
                        }
                        continue;
                    }
                    self.portal_frames[pi].twin = Some(other);
                    self.portal_frames[oi].twin = Some(p);
                    self.portal_frames[pi].connected_sector = Some(b);
                    self.portal_frames[oi].connected_sector = Some(a);
                    self.visibility_incoming_push(a, other)?;
                    self.visibility_incoming_push(b, p)?;
                    break;
                }
            }
            if self.portal(p)?.connected_sector.is_none() {
                let pi = self.portal_index(p)?;
                self.portal_frames[pi].twin = None;
                let pos = portal_vector(self, p, 0x84)?;
                let own = self.portal(p)?.sector;
                let mut destination = None;
                for &s in &sectors {
                    if self.sector_check_point(s, pos)? && Some(s) != own {
                        destination = Some(s);
                        break;
                    }
                }
                let destination = destination.unwrap_or(self.scene(scene)?.sectors[0]);
                self.portal_frames[pi].connected_sector = Some(destination);
                self.visibility_incoming_push(destination, p)?;
            }
        }
        Ok(())
    }
}

pub(crate) struct PortalWalk {
    options: SceneVisibilityOptions,
    view: CameraView,
    stack: Vec<SectorKey>,
    prepared: Vec<crate::occluder_visibility::SectorOcclusion>,
}
impl PortalWalk {
    pub fn new(options: SceneVisibilityOptions, view: CameraView) -> Self {
        Self {
            options,
            view,
            stack: Vec::new(),
            prepared: Vec::new(),
        }
    }
    pub fn walk(
        &mut self,
        arena: &mut ModelArena,
        sector: SectorKey,
        portal: Option<PortalKey>,
        clip: Option<[[f32; 2]; 2]>,
        output: &mut SceneVisibility,
    ) -> Result<()> {
        if arena.state(sector.frame())?.flags & ON == 0 {
            return Ok(());
        }
        let mut partly = false;
        if let Some(p) = portal {
            if arena.portal(p)?.flags() & 4 == 0 {
                return Ok(());
            }
            let delta = vector_sub(portal_vector(arena, p, 0x110)?, self.view.position)?;
            let squared = square(delta)?;
            let radius = portal_scalar(arena, p, 0x11c)?;
            if squared
                > math::add(
                    math::mul(radius, radius)?,
                    crate::scene_visibility::arithmetic::mul_constant(
                        math::mul(self.view.far, self.view.far)?,
                        1.2,
                    )?,
                )?
            {
                return Ok(());
            }
            let range = portal_scalar(arena, p, 0xc)?;
            if range > 0.
                && number(crate::native_precision::div(
                    squared,
                    self.view.fov_correction_2,
                ))? > range
            {
                if arena.portal(p)?.flags() & 0x10 != 0 {
                    return Ok(());
                }
                partly = true;
            }
            let pi = arena.portal_index(p)?;
            let flags = arena.portal_frames[pi].flags() | 8;
            arena.portal_frames[pi].put(8, flags);
            output.portals_passed += 1;
            if arena.portal(p)?.twin.is_none() && arena.portal(p)?.connected_sector == Some(sector)
            {
                for &s in self.stack.iter().rev() {
                    if s == sector {
                        break;
                    }
                    let i = arena.sector_index(s)?;
                    arena.sectors[i].sector_flags |= 0x80;
                }
            }
        }
        let si = arena.sector_index(sector)?;
        if arena.sectors[si].sector_flags & IN_WALK != 0 {
            return Ok(());
        }
        arena.sectors[si].sector_flags |= VISIBLE | IN_WALK;
        if self.stack.len() >= 100 {
            return Err(SceneVisibilityError::Unsupported(
                "portal walk beyond source sector stack",
            ));
        }
        append(&mut self.stack, sector)?;
        if !output.visible_sectors.contains(&sector) {
            append(&mut output.visible_sectors, sector)?;
        }
        if let Some(c) = clip {
            if math::sub(c[1][0], c[0][0])? < 10. * (2. / self.options.viewport[0] as f32)
                && math::sub(c[1][1], c[0][1])? < 10. * (2. / self.options.viewport[1] as f32)
            {
                partly = true;
            }
        }
        let first = arena.sectors[si].sector_flags & CULLED == 0;
        arena.visibility_cull_sector(sector, clip, partly, self.view, self.options, 0, output)?;
        if first {
            let prepared = crate::occluder_visibility::prepare_sector(arena, sector, self.view)?;
            output.occluders_active += prepared.active_count();
            append(&mut self.prepared, prepared)?;
        }
        let outgoing = arena.sector(sector)?.geometry.portals.clone();
        let incoming = arena.sector(sector)?.geometry.incoming_portals.clone();
        for (portals, incoming) in [(&outgoing, false), (&incoming, true)] {
            for &p in portals {
                let portal = arena.portal(p)?;
                if incoming && portal.twin.is_some() {
                    continue;
                }
                let d = portal_distance(arena, p, self.view.position)?;
                if (incoming && d >= 0.) || (!incoming && d <= 0.) || portal.flags() & 2 != 0 {
                    continue;
                }
                if let Some(next_clip) = parse_portal(arena, p, clip, self.view)? {
                    let next = if incoming {
                        arena.portal(p)?.sector
                    } else {
                        arena.portal(p)?.connected_sector
                    }
                    .ok_or(SceneVisibilityError::InvalidRing)?;
                    self.walk(arena, next, Some(p), Some(next_clip), output)?;
                }
            }
        }
        self.stack.pop();
        arena.sectors[si].sector_flags &= !IN_WALK;
        Ok(())
    }
    pub fn process_occluders(
        &mut self,
        arena: &mut ModelArena,
        output: &mut SceneVisibility,
    ) -> Result<()> {
        if !self.options.use_occlusion {
            return Ok(());
        }
        for prepared in &mut self.prepared {
            prepared.cull_visuals(
                arena,
                &mut output.main,
                self.options.render_number,
                &mut output.visuals_occluded,
            )?;
        }
        Ok(())
    }
}

fn parse_portal(
    arena: &mut ModelArena,
    p: PortalKey,
    parent: Option<[[f32; 2]; 2]>,
    view: CameraView,
) -> Result<Option<[[f32; 2]; 2]>> {
    let vertices = portal_vertices(arena, p)?;
    let pi = arena.portal_index(p)?;
    arena.portal_frames[pi].bytes[0x108] = (arena.portal_frames[pi].bytes[0x108] & !1) | 2;
    let mut projected = Vec::new();
    let mut behind = false;
    let mut front = false;
    for v in vertices {
        let m = view.projection.elements;
        // The raw W calculation at 4466d..44683 is Z,Y,X,d. The following
        // XYZ transform uses the separately evidenced operator*2e980 order.
        let w = distance_zyx([m[3], m[7], m[11], m[15]], v)?;
        if w == 0. {
            for (at, value) in [
                (0x138, -1.01f32),
                (0x13c, -1.01),
                (0x140, 0.),
                (0x144, 1.01),
                (0x148, 1.01),
                (0x14c, 0.),
            ] {
                arena.portal_frames[pi].put(at, value.to_bits());
            }
            return Ok(Some([[-1.01; 2], [1.01; 2]]));
        }
        behind |= w < 0.;
        front |= w > 0.;
        if w < 0. {
            arena.portal_frames[pi].bytes[0x108] |= 1;
        } else {
            arena.portal_frames[pi].bytes[0x108] &= !2;
        }
        let reciprocal = number(crate::native_precision::div(1., w))?;
        let pos = math::transformed(&view.projection, v)?;
        append(
            &mut projected,
            [
                math::mul(pos[0], reciprocal)?,
                math::mul(pos[1], reciprocal)?,
                w,
            ],
        )?;
    }
    if !front {
        return Ok(None);
    }
    let mut rect: [[f32; 2]; 2];
    let rect_depth;
    if behind {
        let n = projected.len();
        let start = (0..n)
            .find(|&i| projected[i][2] > 0. && projected[(i + n - 1) % n][2] < 0.)
            .ok_or(SceneVisibilityError::Unsupported(
                "portal projected winding",
            ))?;
        rect = [projected[start][..2].try_into().unwrap(); 2];
        rect_depth = projected[start][2];
        let mut end = start;
        for count in 0..n {
            let i = (start + count) % n;
            if projected[i][2] < 0. {
                break;
            }
            end = i;
            for a in 0..2 {
                if rect[0][a] > projected[i][a] {
                    rect[0][a] = projected[i][a];
                }
                if rect[1][a] < projected[i][a] {
                    rect[1][a] = projected[i][a];
                }
            }
        }
        for axis in 0..2 {
            let d1 = math::sub(projected[start][axis], projected[(start + n - 1) % n][axis])?;
            let d2 = math::sub(projected[end][axis], projected[(end + 1) % n][axis])?;
            if d1 >= 0. || d2 >= 0. {
                rect[1][axis] = 1.01;
            }
            if d1 <= 0. || d2 <= 0. {
                rect[0][axis] = -1.01;
            }
        }
    } else {
        rect = [projected[0][..2].try_into().unwrap(); 2];
        rect_depth = projected[0][2];
        for p in &projected[1..] {
            for a in 0..2 {
                if rect[0][a] > p[a] {
                    rect[0][a] = p[a];
                }
                if rect[1][a] < p[a] {
                    rect[1][a] = p[a];
                }
            }
        }
    }
    for (side, at) in [0x120, 0x12c].into_iter().enumerate() {
        for (axis, value) in rect[side].iter().enumerate() {
            arena.portal_frames[pi].put(at + axis * 4, value.to_bits());
        }
        arena.portal_frames[pi].put(at + 8, rect_depth.to_bits());
    }
    let clip = if let Some(parent) = parent {
        if parent[0][0] > rect[1][0]
            || parent[1][0] < rect[0][0]
            || parent[0][1] > rect[1][1]
            || parent[1][1] < rect[0][1]
        {
            return Ok(None);
        }
        [
            std::array::from_fn(|axis| {
                if parent[0][axis] > rect[0][axis] {
                    parent[0][axis]
                } else {
                    rect[0][axis]
                }
            }),
            std::array::from_fn(|axis| {
                if parent[1][axis] < rect[1][axis] {
                    parent[1][axis]
                } else {
                    rect[1][axis]
                }
            }),
        ]
    } else {
        let clip = rect.map(|v| v.map(|x| x.clamp(-1.01, 1.01)));
        if clip[0][0] == clip[1][0] || clip[0][1] == clip[1][1] {
            return Ok(None);
        }
        clip
    };
    for (side, at) in [0x138, 0x144].into_iter().enumerate() {
        for (axis, value) in clip[side].iter().enumerate() {
            arena.portal_frames[pi].put(at + axis * 4, value.to_bits());
        }
        if parent.is_none() {
            arena.portal_frames[pi].put(at + 8, rect_depth.to_bits());
        }
    }
    Ok(Some(clip))
}
