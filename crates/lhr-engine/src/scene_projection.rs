//! Scene render dispatch, sector-ordered enumeration and screen unmapping,
//! PC RVAs 0x49160/0x49180/0x49a30 (LS3DF.dll SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! Ghidra formed no function at these RVAs (resolve_containing_procedure
//! `not_in_procedure`); behavior below is reconstructed from complete
//! per-address decoded-instruction walks (entry -> RET, branch targets
//! followed), not decompiler pseudocode. Guessed names are working names
//! only (mafia-re I3d_scene.cpp:196-224,369-414); original symbols unknown.
//! Float work uses the crate PC24/RN wrappers; x87 80-bit intermediates
//! wider than f32 stay a disclosed limitation (same class as the existing
//! camera ports).
#![forbid(unsafe_code)]

use crate::frame_transform::FrameError;
use crate::model_loading::{ModelArena, ModelError};
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::native_transform::SMatrix;
use crate::scene_ownership::{SceneKey, SceneOwnerError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneProjectionError<E> {
    Model(ModelError),
    Screen(E),
    NoActiveCamera(SceneKey),
    Camera(crate::frame_camera::CameraError),
    NumericUnavailable(&'static str),
    LengthMismatch { inputs: usize, outputs: usize },
}
impl<E> From<ModelError> for SceneProjectionError<E> {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

/// Backend contract for I3D_scene::Render (RVA 0x49160, contract: 7
/// insns/1 BB ending in an indirect driver tail-call through vtable slot
/// +0x1c; a live D3D8 device is unobtainable on this host, so only the
/// CPU-side ordering — resolve scene, exactly one backend call — is
/// reconstructed here).
pub trait SceneRenderBackend {
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError>;
}

/// Working sector-filter bits from the EnumFrames instruction immediates
/// (TEST ESI,imm); ENUMF_* are upstream working names (mafia-re
/// I3d_scene.cpp:211-221), not evidenced originals.
pub const ENUMF_BACKDROP: u32 = 0x40000;
pub const ENUMF_OMNI: u32 = 0x80000;
pub const ENUMF_NEAR: u32 = 0x100000;
/// Native I3DERR_NOTINITIALIZED returned with no active camera.
pub const NOT_INITIALIZED: u32 = 0x80000009;

/// Unmapped screen point: world position and ray direction.
pub type UnmappedPoint<E> = Result<([f32; 3], [f32; 3]), SceneProjectionError<E>>;

impl ModelArena {
    /// I3D_scene::Render (0x49160): resolve, then delegate once. Errors leave
    /// no partial backend state; a rejecting backend is never retried here.
    pub fn scene_render_with_backend(
        &self,
        scene: SceneKey,
        backend: &mut impl SceneRenderBackend,
    ) -> Result<u32, ModelError> {
        self.scene(scene)?;
        backend.render_scene(self, scene)
    }

    /// I3D_scene::EnumFrames (0x49180, differential: 61 insns/~8 BBs, shared
    /// RET): primary, then backdrop/omni/near gated by flags; a
    /// signed-negative sector result aborts and propagates (native JL). The
    /// near call's result returns directly with no further check. `mask=None`
    /// admits the common NULL-mask call; `Some` is an explicit error
    /// (filtering lives in the unported sector-enum layer).
    pub fn scene_enum_frames(
        &mut self,
        scene: SceneKey,
        user: u32,
        flags: u32,
        mask: Option<&[u8]>,
        mut visit: impl FnMut(crate::frame_transform::FrameKey, u32) -> Result<u32, FrameError>,
    ) -> Result<u32, ModelError> {
        if mask.is_some() {
            return Err(SceneOwnerError::EnumMaskUnavailable.into());
        }
        let sectors = self.scene(scene)?.sectors;
        let mut order = [sectors[0].frame(); 4];
        let mut count = 1;
        if flags & ENUMF_BACKDROP != 0 {
            order[count] = sectors[1].frame();
            count += 1;
        }
        if flags & ENUMF_OMNI != 0 {
            order[count] = sectors[2].frame();
            count += 1;
        }
        let mut ret = 0u32;
        for &root in &order[..count] {
            ret = self.graph.enum_frames(root, |key| visit(key, user))?;
            if (ret as i32) < 0 {
                return Ok(ret);
            }
        }
        if flags & ENUMF_NEAR != 0 {
            ret = self
                .graph
                .enum_frames(sectors[3].frame(), |key| visit(key, user))?;
        }
        Ok(ret)
    }

    /// I3D_scene::UnmapScreenPoint (0x49a30, differential: ~230 insns/~20
    /// BBs, 5 RETs). Camera-first (absent -> NoActiveCamera, the
    /// 0x80000009 word via NOT_INITIALIZED), then screen dims in native
    /// order (sx via IGraph +0x70, sy via +0x74), then the
    /// orthogonal/perspective split on camera byte +0x15c. Ortho:
    /// s=1/ortho_scale (+0x160), pos=(s*fx, -(fy/aspect*s), 0) with aspect
    /// +0x158, dir = world-matrix row 2 normalized. Perspective: pos = world
    /// row 3, v=(fx*ncp_x, -(fy*ncp_y), ncp) with ncp +0x144 and ncp_x/y
    /// +0x3e4/+0x3e8, rotated by view_inv (+0x224), normalized. The
    /// conditional matrix refresh (flags&0x20 gate) runs through
    /// world_matrix, the same producer scene_set_active_camera uses.
    pub fn scene_unmap_screen_point<S: crate::scene_resources::SceneScreen>(
        &mut self,
        scene: SceneKey,
        screen: &mut S,
        x: i32,
        y: i32,
    ) -> UnmappedPoint<S::Error> {
        use SceneProjectionError as Error;
        let camera = self
            .scene_active_camera(scene)
            .map_err(Error::Model)?
            .ok_or(Error::NoActiveCamera(scene))?;
        let sx = screen.screen_sx().map_err(Error::Screen)?;
        let half_x = round_pc24(f64::from(sx) * 0.5);
        let sy = screen.screen_sy().map_err(Error::Screen)?;
        let half_y = round_pc24(f64::from(sy) * 0.5);
        let fx = div(sub(x as f32, half_x), half_x);
        let fy = div(sub(y as f32, half_y), half_y);
        // Conditional matrix refresh first (native flags&0x20 gate inside
        // GetMatrix); projection words are untouched by it, so reading them
        // after keeps native observables while satisfying borrow order.
        let world = self.world_matrix(camera.frame()).map_err(Error::Model)?;
        let cam = self.camera(camera).map_err(Error::Model)?;
        let ortho = cam.orthographic_byte_if_available().ok_or(Error::Camera(
            crate::frame_camera::CameraError::IncompleteProjection(camera),
        ))?;
        if ortho != 0 {
            let scale = cam
                .scalar(0x160)
                .ok_or(Error::NumericUnavailable("ortho scale"))?;
            let aspect = cam
                .scalar(0x158)
                .ok_or(Error::NumericUnavailable("aspect"))?;
            let s = div(1.0, scale);
            let local = [mul(s, fx), -mul(div(fy, aspect), s), 0.0];
            let pos = world.transform_point(local);
            let row: [f32; 3] = world.elements[8..11].try_into().unwrap();
            Ok((pos, normalize_screen_vector(row)))
        } else {
            let pos: [f32; 3] = world.elements[12..15].try_into().unwrap();
            let incomplete = || {
                Error::Camera(crate::frame_camera::CameraError::IncompleteProjection(
                    camera,
                ))
            };
            let ncp = cam.scalar(0x144).ok_or_else(incomplete)?;
            let nx = cam.scalar(0x3e4).ok_or_else(incomplete)?;
            let ny = cam.scalar(0x3e8).ok_or_else(incomplete)?;
            let v = [mul(fx, nx), -mul(fy, ny), ncp];
            let inv = cam.view_inverse_matrix().ok_or_else(incomplete)?;
            Ok((pos, normalize_screen_vector(rotate_by_matrix(v, inv))))
        }
    }
}

/// S_vector::RotateByMatrix association (RVA 0x2ea10), helper phase (no
/// function credit): out[c] = (v1*m[4+c] + v0*m[c]) + v2*m[8+c], no
/// translation (43 insns/1 BB evidenced).
fn rotate_by_matrix(v: [f32; 3], m: SMatrix) -> [f32; 3] {
    std::array::from_fn(|col| {
        add(
            add(mul(v[1], m.elements[4 + col]), mul(v[0], m.elements[col])),
            mul(v[2], m.elements[8 + col]),
        )
    })
}

/// Inline S_vector::Normalize shared by both Unmap branches: sq=(x^2+y^2)+z^2;
/// |sq-1|<1e-8 (double const = f64::from(1e-8f32)) keeps; sq<1e-8 (ordered;
/// NaN falls through to scaling) picks the first nonzero axis X->Z->Y with
/// sign (-1 iff <0 else +1); else 1/sqrt(sq) scaling.
fn normalize_screen_vector(v: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = v;
    let sq = add(add(mul(x, x), mul(y, y)), mul(z, z));
    if f64::from(sub(sq, 1.0)).abs() < f64::from(1e-8f32) {
        return v;
    }
    if sq < 1e-8 {
        let axis = if x != 0.0 {
            0
        } else if z != 0.0 {
            2
        } else {
            1
        };
        let mut out = v;
        out[axis] = if out[axis] < 0.0 { -1.0 } else { 1.0 };
        out
    } else {
        let reciprocal = div(1.0, sqrt(sq));
        [mul(x, reciprocal), mul(y, reciprocal), mul(z, reciprocal)]
    }
}

pub(crate) fn round_pc24(exact: f64) -> f32 {
    let single = exact as f32;
    crate::native_precision::record_precision(f64::from(single) != exact);
    single
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_transform::I3dFrameState;
    use crate::scene_resources::SceneScreen;

    struct Screen;
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> Result<i32, ()> {
            Ok(800)
        }
        fn screen_sy(&mut self) -> Result<i32, ()> {
            Ok(600)
        }
    }
    struct Recorder {
        calls: Vec<SceneKey>,
    }
    impl SceneRenderBackend for Recorder {
        fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
            self.calls.push(scene);
            // Backend-defined failure words pass through opaquely.
            if arena.scene(scene).unwrap().frames().is_empty() {
                return Ok(0);
            }
            Ok(0x4000_0001)
        }
    }

    #[test]
    fn render_resolves_scene_then_delegates_once() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let mut backend = Recorder { calls: Vec::new() };
        assert_eq!(arena.scene_render_with_backend(scene, &mut backend), Ok(0));
        // Exactly one backend call per render; resolution happened first
        // (an unknown scene would fail before any backend effect).
        assert_eq!(backend.calls, [scene]);
        assert_eq!(arena.scene_render_with_backend(scene, &mut backend), Ok(0));
        assert_eq!(backend.calls, [scene, scene]);
    }

    #[test]
    fn enum_frames_visits_sectors_in_order_gates_by_flags_and_propagates_signed_abort() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        for (index, name) in [b"a", b"b", b"c", b"d"].into_iter().enumerate() {
            let key = arena
                .insert(I3dFrameState::dummy(), None, Vec::new(), Vec::new(), None)
                .unwrap();
            arena.set_frame_name(key, Some(name)).unwrap();
            arena.scene_add_frame(scene, key).unwrap();
            arena
                .link_frame(key, Some(sectors[index].frame()), 0)
                .unwrap();
            arena.release(key).unwrap();
        }
        let user = 0x1234;
        let mut order = Vec::new();
        let ret = arena
            .scene_enum_frames(scene, user, 0, None, |key, u| {
                order.push((key, u));
                Ok(0)
            })
            .unwrap();
        assert_eq!(ret, 0);
        assert_eq!(order.len(), 1);
        order.clear();
        arena
            .scene_enum_frames(
                scene,
                user,
                ENUMF_BACKDROP | ENUMF_OMNI | ENUMF_NEAR,
                None,
                |key, u| {
                    order.push((key, u));
                    Ok(0)
                },
            )
            .unwrap();
        assert_eq!(order.len(), 4);
        assert!(order.iter().all(|&(_, u)| u == user));
        // SKIPCHILDREN(1) on childless roots still visits each root once.
        order.clear();
        let ret = arena
            .scene_enum_frames(scene, user, 0x1c0000, None, |key, u| {
                order.push((key, u));
                Ok(1)
            })
            .unwrap();
        assert_eq!(ret, 0);
        assert_eq!(order.len(), 4);
        // CANCELED aborts with propagation after two visits.
        order.clear();
        let ret = arena
            .scene_enum_frames(scene, user, 0x1c0000, None, |key, u| {
                order.push((key, u));
                if order.len() == 2 {
                    Ok(0x8000000d)
                } else {
                    Ok(0)
                }
            })
            .unwrap();
        assert_eq!(ret, 0x8000000d);
        assert_eq!(order.len(), 2);
        // An explicit mask is a fail-closed boundary error with no visits.
        assert!(matches!(
            arena.scene_enum_frames(scene, user, 0, Some(b"name"), |_, _| Ok(0)),
            Err(ModelError::Scene(SceneOwnerError::EnumMaskUnavailable))
        ));
    }

    #[test]
    fn unmap_ortho_center_and_corner_vectors_with_normalize_spots() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let mut screen = Screen;
            let camera = arena.create_camera(&mut screen).unwrap();
            arena.camera_set_orthographic(camera, true).unwrap();
            arena.camera_set_ortho_scale(camera, 2.0).unwrap();
            arena.camera_set_aspect_ratio(camera, 2.0).unwrap();
            arena.scene_set_active_camera(scene, Some(camera)).unwrap();
            arena.camera_update(camera).unwrap();
            // Center: fx=fy=0 -> local origin -> world origin, dir row2.
            let (pos, dir) = arena
                .scene_unmap_screen_point(scene, &mut screen, 400, 300)
                .unwrap();
            assert_eq!(pos, [0., 0., 0.]);
            assert_eq!(dir, [0., 0., 1.]);
            // Corner (800,0): fx=1, fy=-1 -> pos [0.5,0.25,0].
            let (pos, _) = arena
                .scene_unmap_screen_point(scene, &mut screen, 800, 0)
                .unwrap();
            assert_eq!(pos, [0.5, 0.25, 0.]);
            // Residue: unmap after Close keeps working (sector state is
            // irrelevant to the projection inputs).
            assert_eq!(
                arena
                    .scene_unmap_screen_point(scene, &mut screen, 400, 300)
                    .unwrap()
                    .0,
                [0., 0., 0.]
            );
            // No active camera maps to the NOTINITIALIZED word carrier.
            arena.scene_set_active_camera(scene, None).unwrap();
            assert!(matches!(
                arena.scene_unmap_screen_point(scene, &mut screen, 0, 0),
                Err(SceneProjectionError::NoActiveCamera(_))
            ));
            assert_eq!(NOT_INITIALIZED, 0x80000009);
            // Normalize helper spots (helper phase, no function credit).
            assert_eq!(normalize_screen_vector([0., 0., 0.]), [0., 1., 0.]);
            assert_eq!(
                normalize_screen_vector([3., 4., 0.]).map(f32::to_bits),
                [0x3f19999au32, 0x3f4ccccd, 0x00000000]
            );
            assert_eq!(normalize_screen_vector([-2., 0., 0.]), [-1., 0., 0.]);
            assert_eq!(normalize_screen_vector([0., 0., -4.]), [0., 0., -1.]);
            arena.release(camera.frame()).unwrap();
        });
    }

    #[test]
    fn unmap_perspective_center_ray_through_identity_view() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let mut screen = Screen;
            let camera = arena.create_camera(&mut screen).unwrap();
            arena.scene_set_active_camera(scene, Some(camera)).unwrap();
            arena.camera_update(camera).unwrap();
            let (pos, dir) = arena
                .scene_unmap_screen_point(scene, &mut screen, 400, 300)
                .unwrap();
            assert_eq!(pos, [0., 0., 0.]);
            assert_eq!(dir, [0., 0., 1.]);
            assert!(arena
                .camera(camera)
                .unwrap()
                .view_inverse_matrix()
                .is_some());
            arena.release(camera.frame()).unwrap();
        });
    }
}
