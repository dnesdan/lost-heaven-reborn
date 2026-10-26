//! Scene-owned portal and occluder visibility feeding the renderer's frame lists.
//! Reference LS3DF entries 0x45700/0x437f0/0x41df0/0x43000, SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! This is a finite normal-or-zero PC24 source reconstruction, not an original
//! runtime comparison. Unsupported geometry/numerics return errors, never culls.
use crate::frame_camera::CameraKey;
use crate::frame_sector::{math, SectorError};
use crate::frame_transform::{FrameKey, FrameKind};
use crate::mesh_bounds::MeshBoundVolume;
use crate::model_loading::{ModelArena, ModelError};
use crate::native_transform::SMatrix;
use crate::scene_ownership::{SceneKey, SectorKey};
#[path = "scene_visibility_math.rs"]
pub(crate) mod arithmetic;

#[derive(Clone, Copy, Debug)]
pub struct SceneVisibilityOptions {
    pub viewport: [u32; 2],
    /// Supplied by the actual render caller; no parallel driver counter.
    pub render_number: u32,
    pub use_occlusion: bool,
}
#[derive(Debug)]
pub struct SceneVisibility {
    pub camera: CameraKey,
    pub camera_sector: SectorKey,
    pub backdrop: Vec<FrameKey>,
    pub main: Vec<FrameKey>,
    pub near: Vec<FrameKey>,
    pub visible_sectors: Vec<SectorKey>,
    pub portals_passed: usize,
    pub occluders_active: usize,
    pub visuals_occluded: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SceneVisibilityError {
    Model(ModelError),
    NoActiveCamera(SceneKey),
    CameraSceneMismatch,
    CameraData(&'static str),
    InvalidViewport,
    InvalidRing,
    Allocation,
    Unsupported(&'static str),
}
impl From<ModelError> for SceneVisibilityError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
impl From<SectorError> for SceneVisibilityError {
    fn from(e: SectorError) -> Self {
        Self::Model(e.into())
    }
}
impl From<crate::frame_transform::FrameError> for SceneVisibilityError {
    fn from(e: crate::frame_transform::FrameError) -> Self {
        Self::Model(e.into())
    }
}
impl From<crate::frame_occluder::OccluderError> for SceneVisibilityError {
    fn from(e: crate::frame_occluder::OccluderError) -> Self {
        Self::Model(e.into())
    }
}
pub(crate) type Result<T> = std::result::Result<T, SceneVisibilityError>;
pub(crate) type Plane = [f32; 4];
pub(crate) const ON: u32 = 1;
pub(crate) const IN_VIEW: u32 = 0x10000;
pub(crate) const CLIP: u32 = 0x200000;
const NO_CULL: u32 = 0x08000000;
pub(crate) const VISIBLE: u32 = 1;
pub(crate) const CULLED: u32 = 0x10;
pub(crate) const IN_WALK: u32 = 2;

pub(crate) fn number(v: f32) -> Result<f32> {
    if v.is_finite() && !v.is_subnormal() {
        Ok(v)
    } else {
        Err(SceneVisibilityError::Unsupported(
            "visibility numeric exponent",
        ))
    }
}
fn ordered_distance(p: Plane, v: [f32; 3], order: [usize; 3]) -> Result<f32> {
    Ok(math::add(
        math::add(
            math::add(
                math::mul(p[order[0]], v[order[0]])?,
                math::mul(p[order[1]], v[order[1]])?,
            )?,
            math::mul(p[order[2]], v[order[2]])?,
        )?,
        p[3],
    )?)
}
/// Raw CTP sphere tests and ProcessOccluders sum Z,Y,X before adding d.
pub(crate) fn distance_zyx(p: Plane, v: [f32; 3]) -> Result<f32> {
    ordered_distance(p, v, [2, 1, 0])
}
/// Raw ParsePortalsInSector backfaces and outgoing TestVertList side tests.
pub(crate) fn distance_yzx(p: Plane, v: [f32; 3]) -> Result<f32> {
    ordered_distance(p, v, [1, 2, 0])
}
pub(crate) fn distance_xyz(p: Plane, v: [f32; 3]) -> Result<f32> {
    ordered_distance(p, v, [0, 1, 2])
}
pub(crate) fn distance_zxy(p: Plane, v: [f32; 3]) -> Result<f32> {
    ordered_distance(p, v, [2, 0, 1])
}

/// The native inline box test starts with d+X+Y+Z, then updates its running
/// value with corner deltas. Recomputing eight dots changes cancellation.
fn box_reaches_plane(bbox: [[f32; 3]; 2], p: Plane) -> Result<bool> {
    let x = math::mul(p[0], bbox[0][0])?;
    let y = math::mul(p[1], bbox[0][1])?;
    let z = math::mul(p[2], bbox[0][2])?;
    let mut t = math::add(math::add(math::add(p[3], x)?, y)?, z)?;
    if t >= 0. {
        return Ok(true);
    }
    let dz = math::sub(math::mul(p[2], bbox[1][2])?, z)?;
    t = math::add(t, dz)?;
    if t >= 0. {
        return Ok(true);
    }
    let dy = math::sub(math::mul(p[1], bbox[1][1])?, y)?;
    t = math::add(t, dy)?;
    if t >= 0. {
        return Ok(true);
    }
    t = math::sub(t, dz)?;
    if t >= 0. {
        return Ok(true);
    }
    let dx = math::sub(math::mul(p[0], bbox[1][0])?, x)?;
    t = math::add(t, dx)?;
    if t >= 0. {
        return Ok(true);
    }
    t = math::add(t, dz)?;
    if t >= 0. {
        return Ok(true);
    }
    t = math::sub(t, dy)?;
    if t >= 0. {
        return Ok(true);
    }
    Ok(math::sub(t, dz)? >= 0.)
}
pub(crate) fn vector_sub(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3]> {
    Ok([
        math::sub(a[0], b[0])?,
        math::sub(a[1], b[1])?,
        math::sub(a[2], b[2])?,
    ])
}
pub(crate) fn append<T>(items: &mut Vec<T>, item: T) -> Result<()> {
    items
        .try_reserve(1)
        .map_err(|_| SceneVisibilityError::Allocation)?;
    items.push(item);
    Ok(())
}
#[derive(Clone, Copy)]
pub(crate) struct CameraView {
    pub position: [f32; 3],
    pub inverse: SMatrix,
    pub projection: SMatrix,
    pub planes: [Plane; 5],
    pub near: f32,
    pub far: f32,
    pub near_xy: [f32; 2],
    pub fov_correction_2: f32,
}
impl CameraView {
    fn read(arena: &ModelArena, camera: CameraKey, scene: SceneKey, region: usize) -> Result<Self> {
        let cam = arena.camera(camera)?;
        let f = |at| {
            cam.scalar(at)
                .ok_or(SceneVisibilityError::CameraData("camera scalar"))
        };
        let matrix = |at| {
            cam.matrix(at)
                .ok_or(SceneVisibilityError::CameraData("camera matrix"))
        };
        let inverse = matrix(if region == 1 { 0x2a4 } else { 0x224 })?;
        let position = if region == 1 {
            [0.; 3]
        } else {
            // camera_update has already produced the canonical world cache.
            // GetWorldPos reads that cache; an inverse-view round trip can
            // change its translation bits for a rotated camera.
            arena.state(camera.frame())?.world.elements[12..15]
                .try_into()
                .unwrap()
        };
        let mut planes = [[0.; 4]; 5];
        for (i, p) in planes.iter_mut().enumerate() {
            for (j, n) in p[..3].iter_mut().enumerate() {
                *n = f(0x404 + region * 80 + i * 12 + j * 4)?;
            }
            p[3] = f(0x404 + region * 80 + 60 + i * 4)?;
        }
        let correction = number(crate::native_precision::div(
            f32::from_bits(0x3fc90fdb),
            f(0x140)?,
        ))?;
        let (near, far) = if region == 1 {
            let r = arena.scene_backdrop_range(scene)?;
            (r[0], r[1])
        } else if region == 2 {
            (f(0x150)?, f(0x154)?)
        } else {
            (f(0x144)?, f(0x148)?)
        };
        Ok(Self {
            position,
            inverse,
            projection: matrix(if region == 1 {
                0x324
            } else if region == 2 {
                0x3a4
            } else {
                0x1e4
            })?,
            planes,
            near,
            far,
            near_xy: [
                f(if region == 2 { 0x3ec } else { 0x3e4 })?,
                f(if region == 2 { 0x3f0 } else { 0x3e8 })?,
            ],
            fov_correction_2: math::mul(correction, correction)?,
        })
    }
    pub fn portal_planes(&self, clip: [[f32; 2]; 2]) -> Result<[Plane; 5]> {
        let x = math::mul(self.near_xy[0], 100.)?;
        let y = math::mul(self.near_xy[1], 100.)?;
        let z = math::mul(self.near, 100.)?;
        let mut corners = [[0.; 3]; 4];
        for (i, xy) in [
            [clip[0][0], clip[1][1]],
            clip[0],
            clip[1],
            [clip[1][0], clip[0][1]],
        ]
        .into_iter()
        .enumerate()
        {
            for axis in 0..3 {
                let m = self.inverse.elements;
                let center = math::add(math::mul(m[8 + axis], z)?, m[12 + axis])?;
                let a = math::mul(m[axis], x)?;
                let b = math::mul(m[4 + axis], y)?;
                corners[i][axis] = math::add(
                    math::add(center, math::mul(xy[1], b)?)?,
                    math::mul(xy[0], a)?,
                )?;
            }
        }
        Ok([
            self.planes[0],
            math::plane(self.position, corners[0], corners[1])?,
            math::plane(self.position, corners[3], corners[2])?,
            math::plane(self.position, corners[2], corners[0])?,
            math::plane(self.position, corners[1], corners[3])?,
        ])
    }
}

/// Source sphere rejection followed by the eight-corner AABB refinement for
/// spheres crossing side planes. Side sphere equality rejects; box equality
/// survives, including both signed zero representations.
pub(crate) fn bounds_visible(
    bounds: MeshBoundVolume,
    planes: &[Plane; 5],
    near: f32,
    far: f32,
    sides: bool,
) -> Result<(bool, bool)> {
    let r = number(bounds.radius)?;
    for value in bounds
        .center
        .into_iter()
        .chain(bounds.bbox.into_iter().flatten())
    {
        number(value)?;
    }
    let d = distance_zyx(planes[0], bounds.center)?;
    if math::add(d, r)? < near || math::sub(d, r)? > far {
        return Ok((false, false));
    }
    let mut clipped = math::sub(d, near)? <= r || math::sub(far, d)? <= r;
    if sides {
        for &p in &planes[1..] {
            let d = distance_zyx(p, bounds.center)?;
            if d <= -r {
                return Ok((false, false));
            }
            if r > d {
                clipped = true;
                if !box_reaches_plane(bounds.bbox, p)? {
                    return Ok((false, false));
                }
            }
        }
    }
    Ok((true, clipped))
}

impl ModelArena {
    pub fn process_scene_visibility(
        &mut self,
        scene: SceneKey,
        options: SceneVisibilityOptions,
    ) -> Result<SceneVisibility> {
        if options.viewport.contains(&0) {
            return Err(SceneVisibilityError::InvalidViewport);
        }
        let si = self.scene_index(scene)?;
        let camera = self
            .scene_active_camera(scene)?
            .ok_or(SceneVisibilityError::NoActiveCamera(scene))?;
        if self.camera(camera)?.scene != Some(scene) {
            return Err(SceneVisibilityError::CameraSceneMismatch);
        }
        // These are the existing same-arena sector and portal geometry producers.
        let sectors = self.visibility_sector_ring(scene)?;
        for &sector in &sectors {
            self.sector_make_ready(sector)?;
            self.update_frame(sector.frame())?;
        }
        let portals = self.visibility_portal_ring(scene)?;
        for &p in &portals {
            self.update_portal(p)?;
        }
        self.update_portal_interconnection(scene)?;
        crate::occluder_visibility::update_lists(self, scene, &sectors)?;
        self.camera_update(camera)?;
        let camera_sector = self
            .camera(camera)?
            .sector
            .ok_or(SceneVisibilityError::CameraData("camera sector"))?;
        let views = [
            CameraView::read(self, camera, scene, 0)?,
            CameraView::read(self, camera, scene, 1)?,
            CameraView::read(self, camera, scene, 2)?,
        ];
        let special = self.scenes[si].sectors;
        for sector in sectors.iter().copied().chain(special) {
            let i = self.sector_index(sector)?;
            self.sectors[i].sector_flags &= !(VISIBLE | CULLED | 0x80 | IN_WALK);
            self.sectors[i].next_visual = None;
        }
        for &p in &portals {
            let i = self.portal_index(p)?;
            let flags = self.portal_frames[i].flags() & !(8 | 2);
            self.portal_frames[i].put(8, flags);
            self.portal_frames[i].bytes[0x108] = 0;
        }
        let mut output = SceneVisibility {
            camera,
            camera_sector,
            backdrop: Vec::new(),
            main: Vec::new(),
            near: Vec::new(),
            visible_sectors: Vec::new(),
            portals_passed: 0,
            occluders_active: 0,
            visuals_occluded: 0,
        };
        let mut walk = crate::portal_visibility::PortalWalk::new(options, views[0]);
        walk.walk(self, camera_sector, None, None, &mut output)?;
        for (sector, view, pass) in [
            (special[1], views[1], 1),
            (special[2], views[0], 0),
            (special[3], views[2], 2),
        ] {
            self.visibility_cull_sector(sector, None, false, view, options, pass, &mut output)?;
        }
        walk.process_occluders(self, &mut output)?;
        Ok(output)
    }

    // The explicit source CTP context travels together across sector and
    // hierarchy calls; keeping it visible avoids a second mutable scene owner.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn visibility_cull_sector(
        &mut self,
        sector: SectorKey,
        clip: Option<[[f32; 2]; 2]>,
        partly_off: bool,
        view: CameraView,
        options: SceneVisibilityOptions,
        pass: usize,
        output: &mut SceneVisibility,
    ) -> Result<()> {
        let si = self.sector_index(sector)?;
        let first = self.sectors[si].sector_flags & CULLED == 0;
        let planes = if let Some(clip) = clip {
            view.portal_planes(clip)?
        } else {
            view.planes
        };
        if first {
            self.visibility_initialize_children(
                sector.frame(),
                clip.is_some(),
                view,
                &planes,
                options,
                pass,
                output,
            )?;
            self.sectors[si].sector_flags |= CULLED;
        }
        if clip.is_some() {
            self.visibility_cull_children(
                sector.frame(),
                partly_off,
                &planes,
                options,
                pass,
                output,
            )?;
        }
        Ok(())
    }
    /// CTP_EnumProcFirst establishes depth eligibility throughout the hierarchy
    /// before any portal side test. Another portal may expose descendants later
    /// in this same render, even when the first aperture rejects their parent.
    // Mirrors the source first-pass CTP globals plus the owned output list.
    #[allow(clippy::too_many_arguments)]
    fn visibility_initialize_children(
        &mut self,
        root: FrameKey,
        thru_portal: bool,
        view: CameraView,
        planes: &[Plane; 5],
        options: SceneVisibilityOptions,
        pass: usize,
        output: &mut SceneVisibility,
    ) -> Result<()> {
        let mut child = self.links(root)?.child;
        while let Some(frame) = child {
            child = self.links(frame)?.next;
            let state = *self.state(frame)?;
            self.graph.projected_state_mut(frame)?.flags &= !(IN_VIEW | CLIP);
            if state.flags & NO_CULL != 0 || state.flags & ON == 0 {
                continue;
            }
            let bounds = self.hr_bound_volume_w(frame)?;
            // Through a portal the first pass only establishes depth eligibility.
            let (eligible, clip) =
                bounds_visible(bounds, planes, view.near, view.far, !thru_portal)?;
            if !eligible {
                continue;
            }
            if thru_portal {
                self.graph.projected_state_mut(frame)?.flags |= IN_VIEW;
            } else {
                self.visibility_publish(frame, options.render_number, pass, output)?;
            }
            if clip {
                self.graph.projected_state_mut(frame)?.flags |= CLIP;
            }
            self.visibility_initialize_children(
                frame,
                thru_portal,
                view,
                planes,
                options,
                pass,
                output,
            )?;
        }
        Ok(())
    }
    fn visibility_publish(
        &mut self,
        frame: FrameKey,
        render_number: u32,
        pass: usize,
        output: &mut SceneVisibility,
    ) -> Result<()> {
        let state = self.graph.projected_state_mut(frame)?;
        state.render_number = render_number;
        if matches!(
            state.kind,
            FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector
        ) {
            let list = match pass {
                1 => &mut output.backdrop,
                2 => &mut output.near,
                _ => &mut output.main,
            };
            if !list.contains(&frame) {
                append(list, frame)?;
            }
        }
        Ok(())
    }
    /// CTP_EnumProc: side-cull the initialized hierarchy for each aperture.
    /// Already-rendered parents are still walked, allowing disjoint apertures
    /// to contribute different children to one sector's final visual list.
    fn visibility_cull_children(
        &mut self,
        root: FrameKey,
        partly_off: bool,
        planes: &[Plane; 5],
        options: SceneVisibilityOptions,
        pass: usize,
        output: &mut SceneVisibility,
    ) -> Result<()> {
        let mut child = self.links(root)?.child;
        while let Some(frame) = child {
            child = self.links(frame)?.next;
            let state = *self.state(frame)?;
            if state.render_number != options.render_number {
                if state.flags & IN_VIEW == 0 {
                    continue;
                }
                let bounds = self.hr_bound_volume_w(frame)?;
                let (visible, clip) = bounds_visible(bounds, planes, f32::MIN, f32::MAX, true)?;
                if !visible {
                    continue;
                }
                if clip {
                    self.graph.projected_state_mut(frame)?.flags |= CLIP;
                }
                self.visibility_publish(frame, options.render_number, pass, output)?;
            }
            let own_sector = self.state(root)?.kind == FrameKind::Sector
                && self.sector(SectorKey(root))?.sector_flags & 0x100 == 0;
            if !partly_off || own_sector {
                self.visibility_cull_children(frame, partly_off, planes, options, pass, output)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_portal::PortalKey;
    use crate::scene_resources::SceneScreen;
    use std::sync::Arc;

    struct Screen;
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> std::result::Result<i32, ()> {
            Ok(800)
        }
        fn screen_sy(&mut self) -> std::result::Result<i32, ()> {
            Ok(600)
        }
    }
    fn options(number: u32) -> SceneVisibilityOptions {
        SceneVisibilityOptions {
            viewport: [800, 600],
            render_number: number,
            use_occlusion: true,
        }
    }
    fn sector(arena: &mut ModelArena, scene: SceneKey, bounds: [[f32; 3]; 2]) -> SectorKey {
        let s = arena.create_sector_frame().unwrap();
        arena.set_frame_bound_volume(s.frame(), bounds).unwrap();
        arena.sector_add_to_scene_ring(s, Some(scene)).unwrap();
        s
    }
    fn portal(arena: &mut ModelArena, s: SectorKey, z: f32, reverse: bool) -> PortalKey {
        let p = arena.create_portal([0; 0x150]).unwrap();
        let mut v = [[-2., -2., z], [-2., 2., z], [2., 2., z], [2., -2., z]];
        if reverse {
            v.reverse();
        }
        let mut bytes = vec![4];
        bytes.extend(4u32.to_le_bytes());
        bytes.extend([0; 24]);
        for v in v {
            for x in v {
                bytes.extend(x.to_le_bytes());
            }
        }
        arena.portal_load_image(p, &bytes).unwrap();
        let pi = arena.portal_index(p).unwrap();
        arena.portal_frames[pi].sector = Some(s);
        let si = arena.sector_index(s).unwrap();
        arena.sectors[si].geometry.portals.push(p);
        arena.update_portal(p).unwrap();
        p
    }
    fn visual(
        arena: &mut ModelArena,
        scene: SceneKey,
        s: SectorKey,
        position: [f32; 3],
        installation: &lhr_assets::Installation,
    ) -> FrameKey {
        let mut source = crate::model_loading::tests::frame(1, None, position);
        source.render_flags = 0;
        source.logic_flags = 0;
        source.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-0.5, -0.5, -0.5], [0.5, 0.5, 0.5], [0., 0., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: lhr_assets::UNASSIGNED_MATERIAL,
                indices: vec![0, 1, 2],
            }],
        });
        let model = arena.create_model().unwrap();
        arena.scene_add_frame(scene, model.root()).unwrap();
        arena.link_frame(model.root(), Some(s.frame()), 0).unwrap();
        let frames = arena
            .load_parsed_into_model(
                model,
                Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames: vec![source],
                }),
            )
            .unwrap();
        let key = frames.frames()[0];
        arena
            .load_plain_visual_resources(model, installation)
            .unwrap();
        key
    }
    #[test]
    fn canonical_scene_portals_select_render_frames_and_camera_motion_changes_list() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let dir = std::env::temp_dir().join(format!(
                "lhr-visibility-fixture-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&dir).unwrap();
            let installation = lhr_assets::Installation::open(&dir).unwrap();
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let a = sector(&mut arena, scene, [[-10., -10., -5.], [10., 10., 5.]]);
            let b = sector(&mut arena, scene, [[-10., -10., 5.], [10., 10., 15.]]);
            let disconnected = sector(&mut arena, scene, [[20., -10., 5.], [30., 10., 15.]]);
            let pa = portal(&mut arena, a, 5., false);
            let pb = portal(&mut arena, b, 5., true);
            let inside = visual(&mut arena, scene, b, [0., 0., 10.], &installation);
            let off_portal = visual(&mut arena, scene, b, [8., 0., 10.], &installation);
            let isolated = visual(
                &mut arena,
                scene,
                disconnected,
                [25., 0., 10.],
                &installation,
            );
            let camera = arena.create_camera(&mut Screen).unwrap();
            arena.scene_set_active_camera(scene, Some(camera)).unwrap();
            let visible = arena.process_scene_visibility(scene, options(1)).unwrap();
            assert_eq!(visible.camera, camera);
            assert_eq!(visible.camera_sector, a);
            assert!(visible.main.contains(&inside));
            assert!(!visible.main.contains(&off_portal));
            assert!(!visible.main.contains(&isolated));
            assert_eq!(arena.portal(pa).unwrap().twin, Some(pb));
            assert_eq!(arena.portal(pb).unwrap().twin, Some(pa));
            assert_eq!(arena.sector(b).unwrap().geometry.incoming_portals, vec![pa]);
            // The live camera's sector and list change without rebuilding a scene.
            arena.camera_set_position(camera, [8., 0., 8.]).unwrap();
            let visible = arena.process_scene_visibility(scene, options(2)).unwrap();
            assert_eq!(visible.camera_sector, b);
            assert!(!visible.main.contains(&inside));
            assert!(visible.main.contains(&off_portal));
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
    #[test]
    fn incoming_portal_allocation_failure_retains_stores_and_is_not_replayed_as_success() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let a = sector(&mut arena, scene, [[-10.; 3], [10.; 3]]);
        let b = sector(&mut arena, scene, [[-10.; 3], [10.; 3]]);
        let pa = portal(&mut arena, a, 5., false);
        let pb = portal(&mut arena, b, 5., true);
        let call = arena.sector_storage().allocation_calls() + 1;
        arena.sector_storage_mut().fail_on_allocation(Some(call));
        assert!(arena.update_portal_interconnection(scene).is_err());
        assert_eq!(arena.portal(pa).unwrap().twin, Some(pb));
        assert_eq!(arena.portal(pa).unwrap().connected_sector, Some(b));
        assert!(arena
            .sector(a)
            .unwrap()
            .geometry
            .incoming_portals
            .is_empty());
        assert_eq!(arena.sector(a).unwrap().geometry.incoming_capacity, 32);
        assert!(arena.update_portal_interconnection(scene).is_err());
        assert!(arena.sector_make_ready(a).is_err());
    }
    #[test]
    fn canonical_occluder_shadow_hides_visual_and_disabling_it_restores_the_draw() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let dir = std::env::temp_dir().join(format!(
                "lhr-occlusion-fixture-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&dir).unwrap();
            let installation = lhr_assets::Installation::open(&dir).unwrap();
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let primary = arena.scene(scene).unwrap().sectors[0];
            let hidden = visual(&mut arena, scene, primary, [0., 0., 10.], &installation);
            let beside = visual(&mut arena, scene, primary, [3., 0., 10.], &installation);
            let occluder = arena.create_occluder().unwrap();
            assert_eq!(
                arena
                    .occluder_build(
                        occluder,
                        &[[-1., -1., 4.], [-1., 1., 4.], [1., 0., 4.]],
                        &[[0, 1, 2], [2, 1, 0]]
                    )
                    .unwrap(),
                0
            );
            arena.scene_add_frame(scene, occluder.frame()).unwrap();
            arena
                .link_frame(occluder.frame(), Some(primary.frame()), 0)
                .unwrap();
            let camera = arena.create_camera(&mut Screen).unwrap();
            arena.scene_set_active_camera(scene, Some(camera)).unwrap();
            let visible = arena.process_scene_visibility(scene, options(1)).unwrap();
            assert!(!visible.main.contains(&hidden));
            assert!(visible.main.contains(&beside));
            assert_eq!(visible.visuals_occluded, 1);
            assert_eq!(visible.occluders_active, 1);
            assert_eq!(arena.state(hidden).unwrap().render_number, 0);
            arena.set_frame_on(occluder.frame(), 0).unwrap();
            let visible = arena.process_scene_visibility(scene, options(2)).unwrap();
            assert!(visible.main.contains(&hidden));
            assert!(visible.main.contains(&beside));
            assert_eq!(visible.visuals_occluded, 0);
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
    #[test]
    fn second_aperture_into_same_sector_reuses_first_depth_initialized_descendants() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let dir = std::env::temp_dir().join(format!(
                "lhr-two-apertures-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&dir).unwrap();
            let installation = lhr_assets::Installation::open(&dir).unwrap();
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let a = sector(&mut arena, scene, [[-10., -10., -5.], [10., 10., 5.]]);
            let b = sector(&mut arena, scene, [[-10., -10., 5.], [10., 10., 15.]]);
            let left = portal(&mut arena, a, 5., false);
            let right = portal(&mut arena, a, 5., false);
            for (p, center) in [(left, -1.), (right, 1.)] {
                let pi = arena.portal_index(p).unwrap();
                for i in 0..4 {
                    let x =
                        f32::from_bits(arena.portal_frames[pi].known_word(0x24 + i * 12).unwrap());
                    arena.portal_frames[pi].put(0x24 + i * 12, (center + x * 0.125).to_bits());
                }
                arena.update_portal(p).unwrap();
            }
            let target = visual(&mut arena, scene, b, [2., 0., 10.], &installation);
            let camera = arena.create_camera(&mut Screen).unwrap();
            arena.scene_set_active_camera(scene, Some(camera)).unwrap();
            let ri = arena.portal_index(right).unwrap();
            arena.portal_frames[ri].put(8, 0);
            let first = arena.process_scene_visibility(scene, options(1)).unwrap();
            assert!(!first.main.contains(&target));
            assert_ne!(
                arena.state(target).unwrap().flags & IN_VIEW,
                0,
                "first pass must initialize descendants even beyond the first aperture"
            );
            arena.portal_frames[ri].put(8, 4);
            let both = arena.process_scene_visibility(scene, options(2)).unwrap();
            assert!(both.main.contains(&target));
            assert_eq!(both.visible_sectors.iter().filter(|&&s| s == b).count(), 1);
            assert!(both.portals_passed >= 2);
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
    #[test]
    fn side_plane_ties_refine_real_box_and_numeric_failure_is_not_invisibility() {
        let planes = [
            [0., 0., 1., 0.],
            [1., 0., 0., 0.],
            [0., 0., 0., 10.],
            [0., 0., 0., 10.],
            [0., 0., 0., 10.],
        ];
        let bounds = MeshBoundVolume {
            bbox: [[-2., -1., 4.], [0., 1., 6.]],
            center: [-1., 0., 5.],
            radius: 2.,
        };
        assert_eq!(
            bounds_visible(bounds, &planes, 1., 10., true).unwrap(),
            (true, true)
        );
        let bad = MeshBoundVolume {
            radius: f32::NAN,
            ..bounds
        };
        assert!(bounds_visible(bad, &planes, 1., 10., true).is_err());
        let outside = MeshBoundVolume {
            bbox: [[-3., -1., 4.], [-1., 1., 6.]],
            ..bounds
        };
        assert!(!bounds_visible(outside, &planes, 1., 10., true).unwrap().0);
    }
    #[test]
    fn raw_ctp_zyx_cancellation_preserves_depth_visibility_branch() {
        let p = [2f32 / 3., 2f32 / 3., -1f32 / 3., 0.];
        let center = [25_165_824., 1.5, 50_331_648.];
        assert_eq!(distance_xyz(p, center).unwrap(), 0.);
        assert_eq!(distance_zyx(p, center).unwrap(), 1.);
        let bounds = MeshBoundVolume {
            bbox: [center; 2],
            center,
            radius: 0.,
        };
        let mut planes = [[0.; 4]; 5];
        planes[0] = p;
        assert_eq!(
            bounds_visible(bounds, &planes, 0.5, 2., false).unwrap(),
            (true, false)
        );
        // The inline AABB path adds d before the products and reuses its
        // corner sum. A general XYZ dot would reject this boundary point.
        let plane = [p[0], p[1], p[2], -0.5];
        assert_eq!(distance_xyz(plane, center).unwrap(), -0.5);
        assert!(box_reaches_plane([center; 2], plane).unwrap());
    }
    #[test]
    fn rotated_camera_visibility_reuses_world_position_sector_lod_and_listener() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let positions = [
                [10., 5., -10.],
                [1234.5, -987.25, 321.125],
                [3.25, 17.75, -23.5],
            ];
            let sectors = positions.map(|position| {
                crate::scene_lightness::loaded_sector_fixture(&mut arena, scene, position)
            });
            let camera = arena.create_camera(&mut Screen).unwrap();
            arena.scene_set_active_camera(scene, Some(camera)).unwrap();
            let mut roundtrip_differs = false;
            for (position, sector) in positions.into_iter().zip(sectors) {
                for direction in [[-1., -0.1, -1.], [1., 2., 3.], [3., -2., 1.]] {
                    arena.camera_set_position(camera, position).unwrap();
                    arena.camera_set_direction(camera, direction, 0.).unwrap();
                    arena.camera_update(camera).unwrap();
                    let world: [f32; 3] = arena.state(camera.frame()).unwrap().world.elements
                        [12..15]
                        .try_into()
                        .unwrap();
                    let inverse: [f32; 3] = arena
                        .camera(camera)
                        .unwrap()
                        .matrix(0x224)
                        .unwrap()
                        .elements[12..15]
                        .try_into()
                        .unwrap();
                    roundtrip_differs |= inverse.map(f32::to_bits) != world.map(f32::to_bits);
                    assert_eq!(world.map(f32::to_bits), position.map(f32::to_bits));
                    assert_eq!(arena.camera(camera).unwrap().sector, Some(sector));
                    for region in [0, 2] {
                        let view = CameraView::read(&arena, camera, scene, region).unwrap();
                        assert_eq!(view.position.map(f32::to_bits), world.map(f32::to_bits));
                        assert_eq!(
                            arena.scene_get_sector(scene, view.position, None).unwrap(),
                            sector
                        );
                    }
                    assert_eq!(
                        CameraView::read(&arena, camera, scene, 1).unwrap().position,
                        [0.; 3]
                    );
                    assert_eq!(
                        crate::scene_render::active_lod_view(&mut arena, scene, 1.)
                            .unwrap()
                            .camera_pc
                            .map(f32::to_bits),
                        world.map(f32::to_bits)
                    );
                    assert_eq!(
                        crate::scene_render::active_listener(&mut arena, scene)
                            .unwrap()
                            .0
                            .map(f32::to_bits),
                        world.map(f32::to_bits)
                    );
                }
            }
            assert!(
                roundtrip_differs,
                "fixture must expose the inverse-view reconstruction error"
            );
        });
    }
}
