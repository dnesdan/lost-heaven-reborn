//! Original LS3DF camera, PC ctor0xb0c0/dtor0xb270/projection0xb400.
//! SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! CameraKey identifies the existing ModelArena frame; unknown native words
//! remain unavailable rather than becoming invented constructor values.
use crate::frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameState};
use crate::model_loading::{ModelArena, ModelError};
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::native_transform::{SMatrix, SQuat};
use crate::scene_ownership::{SceneKey, SectorKey};
use crate::scene_resources::SceneScreen;

pub const CAMERA_WORDS: usize = 0x4f4 / 4;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CameraKey {
    frame: FrameKey,
    reference: crate::script_scheduler::FrameReferenceId,
}
impl CameraKey {
    pub fn frame(self) -> FrameKey {
        self.frame
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CameraError {
    ForeignArena,
    UnknownCamera(FrameKey),
    NumericUnavailable(&'static str),
    IncompleteProjection(CameraKey),
    IncompleteConstruction(CameraKey),
    SceneGeometryUnavailable(SceneKey),
}
impl From<CameraError> for ModelError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}
#[derive(Debug)]
pub enum CameraCreateError<E> {
    DriverContext,
    Model(ModelError),
    Screen(E),
}
#[derive(Debug)]
pub struct CameraCreateFailure<E> {
    pub camera: Option<CameraKey>,
    pub error: CameraCreateError<E>,
}
#[derive(Debug)]
pub struct CameraFrame {
    pub key: CameraKey,
    pub scene: Option<SceneKey>,
    pub sector: Option<SectorKey>,
    words: [Option<u32>; CAMERA_WORDS],
    projection_complete: bool,
    constructor_complete: bool,
    orthographic: u8,
    view_incomplete: bool,
}
impl CameraFrame {
    pub fn word(&self, offset: usize) -> Option<u32> {
        if self.view_incomplete
            && ((0x164..0x1a4).contains(&offset)
                || (0x1e4..0x2e4).contains(&offset)
                || (0x324..0x364).contains(&offset)
                || (0x3a4..0x3e4).contains(&offset)
                || (0x404..0x4f4).contains(&offset))
        {
            return None;
        }
        if !self.projection_complete
            && ((0x1a4..0x1e4).contains(&offset)
                || (0x2e4..0x324).contains(&offset)
                || (0x364..0x3f4).contains(&offset))
        {
            return None;
        }
        if offset.is_multiple_of(4) {
            self.words.get(offset / 4).copied().flatten()
        } else {
            None
        }
    }
    pub fn scalar(&self, offset: usize) -> Option<f32> {
        self.word(offset).map(f32::from_bits)
    }
    pub fn matrix(&self, offset: usize) -> Option<SMatrix> {
        for i in 0..16 {
            self.word(offset + i * 4)?;
        }
        self.matrix_raw(offset)
    }
    /// Current inverse view consumed by Billboard render preparation.
    pub fn view_inverse_matrix(&self) -> Option<SMatrix> {
        self.matrix(0x224)
    }
    fn matrix_raw(&self, offset: usize) -> Option<SMatrix> {
        let mut out = SMatrix::identity();
        for (i, value) in out.elements.iter_mut().enumerate() {
            *value = f32::from_bits(*self.words.get(offset / 4 + i)?.as_ref()?);
        }
        Some(out)
    }
    /// Ready-camera convenience; incomplete constructor observations use the
    /// optional accessor instead of inventing the unwritten byte.
    pub fn orthographic_byte(&self) -> u8 {
        self.orthographic_byte_if_available()
            .expect("camera orthographic byte not yet constructed")
    }
    pub fn orthographic_byte_if_available(&self) -> Option<u8> {
        self.words[0x160 / 4].map(|_| self.orthographic)
    }
    pub fn projection_complete(&self) -> bool {
        self.projection_complete
    }
    fn put(&mut self, offset: usize, value: f32) {
        self.words[offset / 4] = Some(value.to_bits());
    }
    fn put_word(&mut self, offset: usize, value: u32) {
        if offset == 0x15c {
            self.orthographic = value as u8;
            return;
        }
        self.words[offset / 4] = Some(value);
    }
    fn put_matrix(&mut self, offset: usize, m: SMatrix) {
        for (i, v) in m.elements.into_iter().enumerate() {
            self.put(offset + i * 4, v);
        }
    }
    fn f(&self, offset: usize) -> f32 {
        self.scalar(offset)
            .expect("camera constructor-known scalar")
    }
    pub(crate) fn clear_sector(&mut self) {
        self.sector = None;
        self.put_word(0x3f8, 0);
    }
    fn clear_scene(&mut self) {
        self.scene = None;
        self.put_word(0x3f4, 0);
        self.clear_sector();
    }
    fn new(key: CameraKey) -> Self {
        let mut out = Self {
            key,
            scene: None,
            sector: None,
            words: [None; CAMERA_WORDS],
            projection_complete: false,
            constructor_complete: false,
            orthographic: 0,
            view_incomplete: false,
        };
        for (offset, bits) in [
            (0x140, 0x3f490fdb),
            (0x14c, 0x3f490fdb),
            (0x144, 0x3f800000),
            (0x148, 0x447a0000),
            (0x150, 0x3c23d70a),
            (0x154, 0x41a00000),
        ] {
            out.put_word(offset, bits);
        }
        out
    }
    fn finish_constructor_fields(&mut self) {
        for (offset, bits) in [
            (0x15c, 0),
            (0x160, 0x3f800000),
            (0x3e4, 0x3f800000),
            (0x3e8, 0x3f800000),
            (0x3ec, 0x3c23d70a),
            (0x3f0, 0x3c23d70a),
            (0x3f4, 0),
            (0x3f8, 0),
            (0x3fc, 0),
            (0x400, 0),
        ] {
            self.put_word(offset, bits);
        }
    }
    pub fn constructor_complete(&self) -> bool {
        self.constructor_complete
    }
    fn update_projection(&mut self, backdrop: Option<[f32; 2]>) -> Result<(), CameraError> {
        self.projection_complete = false;
        self.put_matrix(0x1a4, SMatrix { elements: [0.; 16] });
        let (n, f, nn, nf, aspect) = (
            self.f(0x144),
            self.f(0x148),
            self.f(0x150),
            self.f(0x154),
            self.f(0x158),
        );
        let checked = |v: f32| {
            if v.is_finite() && (v == 0. || v.is_normal()) {
                Ok(v)
            } else {
                Err(CameraError::NumericUnavailable(
                    "camera projection arithmetic",
                ))
            }
        };
        if u32::from(self.orthographic) != 0 {
            let scale = self.f(0x160);
            let r = checked(div(1., sub(f, n)))?;
            let mut main = SMatrix { elements: [0.; 16] };
            main.elements[0] = scale;
            main.elements[5] = checked(mul(aspect, scale))?;
            main.elements[10] = r;
            main.elements[14] = -mul(r, n);
            main.elements[15] = 1.;
            self.put_matrix(0x1a4, main);
            let (r1, translation) = if let Some([bn, bf]) = backdrop {
                let r = checked(div(1., sub(bf, bn)))?;
                (r, -mul(r, bn))
            } else {
                let r = checked(div(1., sub(mul(f, 1000.), mul(n, 10.))))?;
                (r, mul(mul(r, n), -10.))
            };
            let mut bd = main;
            bd.elements[10] = r1;
            bd.elements[14] = translation;
            self.put_matrix(0x2e4, bd);
            let mut near = main;
            near.elements[14] = -mul(r1, nn);
            self.put_matrix(0x364, near);
        } else {
            let perspective = |this: &mut Self,
                               fov_offset: usize,
                               near: f32,
                               far: f32,
                               offset: usize,
                               x_offset: usize|
             -> Result<(), CameraError> {
                let angle = mul(this.f(fov_offset), 0.5);
                let w = crate::native_transform::native_trig::camera_cotangent(angle)
                    .map_err(|_| CameraError::NumericUnavailable("camera retained cotangent"))?;
                let h = checked(mul(w, aspect))?;
                this.put(offset, w);
                this.put(offset + 20, h);
                let q = checked(div(far, sub(far, near)))?;
                let mut matrix = SMatrix { elements: [0.; 16] };
                matrix.elements[0] = w;
                matrix.elements[5] = h;
                matrix.elements[10] = q;
                matrix.elements[11] = 1.;
                matrix.elements[14] = -mul(q, near);
                this.put_matrix(offset, matrix);
                this.put(x_offset, checked(div(near, w))?);
                this.put(x_offset + 4, checked(div(near, h))?);
                Ok(())
            };
            perspective(self, 0x140, n, f, 0x1a4, 0x3e4)?;
            let mut bd = self.matrix_raw(0x1a4).unwrap();
            let (q, t) = if let Some([bn, bf]) = backdrop {
                let q = checked(div(bf, sub(bf, bn)))?;
                (q, -mul(q, bn))
            } else {
                let far = mul(f, 1000.);
                let q = checked(div(far, sub(far, mul(n, 10.))))?;
                (q, mul(mul(q, n), -10.))
            };
            bd.elements[10] = q;
            bd.elements[14] = t;
            self.put_matrix(0x2e4, bd);
            self.put_matrix(0x364, SMatrix { elements: [0.; 16] });
            perspective(self, 0x14c, nn, nf, 0x364, 0x3ec)?;
        }
        for range in [0x1a4..0x1e4, 0x2e4..0x324, 0x364..0x3a4, 0x3e4..0x3f4] {
            for offset in range.step_by(4) {
                let value = f32::from_bits(self.words[offset / 4].unwrap());
                if !value.is_finite() || value != 0. && !value.is_normal() {
                    return Err(CameraError::NumericUnavailable(
                        "camera projection exponent",
                    ));
                }
            }
        }
        self.projection_complete = true;
        Ok(())
    }
}
/// Masked-invalid FLD single -> FSTP single preserves the NaN sign/payload
/// and sets the quiet bit. This models only the stored bits; invalid-exception
/// status is outside Pc24PrecisionState, and this store does not raise PE.
fn quiet_fld_fstp_single(value: f32) -> f32 {
    let bits = value.to_bits();
    if bits & 0x7f80_0000 == 0x7f80_0000 && bits & 0x007f_ffff != 0 {
        f32::from_bits(bits | 0x0040_0000)
    } else {
        value
    }
}

impl ModelArena {
    pub fn create_camera<S: SceneScreen>(
        &mut self,
        screen: &mut S,
    ) -> Result<CameraKey, CameraCreateFailure<S::Error>> {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Camera;
        let frame = self
            .insert(state, None, Vec::new(), Vec::new(), None)
            .map_err(|error| CameraCreateFailure {
                camera: None,
                error: CameraCreateError::Model(error),
            })?;
        let key = CameraKey {
            frame,
            reference: self.lookup_reference(frame),
        };
        self.cameras.push(CameraFrame::new(key));
        let result = (|| {
            let width = screen.screen_sx().map_err(CameraCreateError::Screen)?;
            let sx = width as f32;
            crate::native_precision::record_precision(f64::from(sx) != f64::from(width));
            let height = screen.screen_sy().map_err(CameraCreateError::Screen)?;
            // FDIVR rounds directly to PC24, not first to binary64. Width
            // already passed through FST32; height remains exact signed FILD32.
            let aspect = crate::native_precision::pc24_integer_ratio(sx as i64, height);
            self.camera_mut(key)
                .map_err(CameraCreateError::Model)?
                .finish_constructor_fields();
            self.camera_mut(key)
                .map_err(CameraCreateError::Model)?
                .put(0x158, aspect);
            self.graph
                .change_flags(key.frame(), 0, 0x08000000)
                .map_err(ModelError::from)
                .map_err(CameraCreateError::Model)?;
            self.camera_update_projection(key)
                .map_err(CameraCreateError::Model)?;
            let camera = self.camera_mut(key).map_err(CameraCreateError::Model)?;
            for offset in [0x164, 0x224, 0x264, 0x2a4] {
                camera.put_matrix(offset, SMatrix::identity());
            }
            for (dst, src) in [(0x1e4, 0x1a4), (0x324, 0x2e4), (0x3a4, 0x364)] {
                camera.put_matrix(dst, camera.matrix(src).unwrap());
            }
            camera.constructor_complete = true;
            Ok(())
        })();
        result.map(|_| key).map_err(|error| CameraCreateFailure {
            camera: Some(key),
            error,
        })
    }
    fn require_camera_ready(&self, key: CameraKey) -> Result<(), ModelError> {
        if !self.camera(key)?.constructor_complete {
            return Err(CameraError::IncompleteConstruction(key).into());
        }
        Ok(())
    }
    pub fn camera_key(&self, frame: FrameKey) -> Result<CameraKey, ModelError> {
        self.frame(frame)?;
        self.cameras
            .iter()
            .find(|camera| camera.key.frame() == frame)
            .map(|camera| camera.key)
            .ok_or_else(|| CameraError::UnknownCamera(frame).into())
    }
    pub fn camera(&self, key: CameraKey) -> Result<&CameraFrame, ModelError> {
        if self.lookup_reference(key.frame()) != key.reference {
            return Err(CameraError::ForeignArena.into());
        }
        self.frame(key.frame())?;
        self.cameras
            .iter()
            .find(|camera| camera.key == key)
            .ok_or_else(|| CameraError::UnknownCamera(key.frame()).into())
    }
    pub(crate) fn camera_mut(&mut self, key: CameraKey) -> Result<&mut CameraFrame, ModelError> {
        if self.lookup_reference(key.frame()) != key.reference {
            return Err(CameraError::ForeignArena.into());
        }
        self.frame(key.frame())?;
        self.cameras
            .iter_mut()
            .find(|camera| camera.key == key)
            .ok_or_else(|| CameraError::UnknownCamera(key.frame()).into())
    }
    pub fn camera_update_projection(&mut self, key: CameraKey) -> Result<(), ModelError> {
        if self.camera(key)?.word(0x158).is_none() || self.camera(key)?.word(0x160).is_none() {
            return Err(CameraError::IncompleteConstruction(key).into());
        }
        let backdrop = self
            .camera(key)?
            .scene
            .map(|scene| self.scene(scene).map(|scene| scene.backdrop_range))
            .transpose()?;
        self.camera_mut(key)?.update_projection(backdrop)?;
        self.graph.camera_projection_changed(key.frame())?;
        Ok(())
    }
    pub fn camera_set_fov(&mut self, key: CameraKey, value: f32) -> Result<(), ModelError> {
        self.camera_set_fov_field(key, value, 0x140)
    }
    pub fn camera_set_near_fov(&mut self, key: CameraKey, value: f32) -> Result<(), ModelError> {
        self.camera_set_fov_field(key, value, 0x14c)
    }
    fn camera_set_fov_field(
        &mut self,
        key: CameraKey,
        mut value: f32,
        offset: usize,
    ) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        if value < 0.01 || value >= f32::from_bits(0x40490fdb) {
            value = if value < 0.01 {
                0.01
            } else {
                f32::from_bits(0x40490fdb)
            };
        }
        self.camera_mut(key)?.put(offset, value);
        if u32::from(self.camera(key)?.orthographic) == 0 {
            self.camera_update_projection(key)?;
        }
        Ok(())
    }
    pub fn camera_set_range(
        &mut self,
        key: CameraKey,
        near: f32,
        far: f32,
    ) -> Result<(), ModelError> {
        self.camera_set_range_field(key, near, far, 0x144)
    }
    pub fn camera_set_near_range(
        &mut self,
        key: CameraKey,
        near: f32,
        far: f32,
    ) -> Result<(), ModelError> {
        self.camera_set_range_field(key, near, far, 0x150)
    }
    fn camera_set_range_field(
        &mut self,
        key: CameraKey,
        near: f32,
        far: f32,
        offset: usize,
    ) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera(key)?;
        if f64::from(sub(far, near).abs()) < 0.01_f64
            || f64::from(near) < 0.001
            || f64::from(far) < 0.001
        {
            return Ok(());
        }
        let camera = self.camera_mut(key)?;
        // b7ed/b7f5 (and b87d/b885): only the near value passes
        // through FLD/FSTP. Far is copied directly with MOV.
        camera.put(offset, quiet_fld_fstp_single(near));
        camera.put(offset + 4, far);
        self.camera_update_projection(key)
    }
    pub fn camera_set_orthographic(
        &mut self,
        key: CameraKey,
        enabled: bool,
    ) -> Result<(), ModelError> {
        self.camera_set_orthographic_byte(key, u8::from(enabled))
    }
    pub fn camera_set_orthographic_byte(
        &mut self,
        key: CameraKey,
        value: u8,
    ) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera_mut(key)?.put_word(0x15c, u32::from(value));
        self.camera_update_projection(key)
    }
    pub fn camera_set_ortho_scale(&mut self, key: CameraKey, value: f32) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera_mut(key)?.put(0x160, value);
        if u32::from(self.camera(key)?.orthographic) != 0 {
            self.camera_update_projection(key)?;
        }
        Ok(())
    }
    pub fn camera_set_aspect_ratio(
        &mut self,
        key: CameraKey,
        value: f32,
    ) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera(key)?;
        if f64::from(value) < 0.01 || value > 100. {
            return Ok(());
        }
        // b976/b980 stores through FLD/FSTP, unlike FOV/ortho MOV.
        self.camera_mut(key)?
            .put(0x158, quiet_fld_fstp_single(value));
        if u32::from(self.camera(key)?.orthographic) == 0 {
            self.camera_update_projection(key)?;
        }
        Ok(())
    }
    pub fn camera_set_position(
        &mut self,
        key: CameraKey,
        value: [f32; 3],
    ) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera(key)?;
        self.graph.assign_position(key.frame(), value)?;
        Ok(())
    }
    pub fn camera_set_direction(
        &mut self,
        key: CameraKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera(key)?;
        let rotation = SQuat::try_make_frame_direction(direction, roll)
            .map_err(|_| CameraError::NumericUnavailable("frame SetDir"))?;
        self.graph.apply_evaluated_rotation(key.frame(), rotation)?;
        Ok(())
    }
    pub fn scene_active_camera(&self, scene: SceneKey) -> Result<Option<CameraKey>, ModelError> {
        Ok(self.scene(scene)?.current_camera)
    }
    pub fn scene_set_active_camera(
        &mut self,
        scene: SceneKey,
        camera: Option<CameraKey>,
    ) -> Result<(), ModelError> {
        let index = self.scene_index(scene)?;
        if let Some(camera) = camera {
            if !self.camera(camera)?.constructor_complete {
                return Err(CameraError::IncompleteConstruction(camera).into());
            }
        }
        if self.scenes[index].current_camera == camera {
            return Ok(());
        }
        if let Some(old) = self.scenes[index].current_camera {
            self.camera_mut(old)?.clear_scene();
            self.release(old.frame())?;
        }
        self.scenes[index].current_camera = camera;
        if let Some(camera) = camera {
            self.camera(camera)?;
            self.retain(camera.frame())?;
            let state = self.camera_mut(camera)?;
            state.scene = Some(scene);
            state.words[0x3f4 / 4] = None;
            let position = self.world_matrix(camera.frame())?.elements[12..15]
                .try_into()
                .unwrap();
            let sector = Some(self.scene_get_sector(scene, position, None)?);
            let state = self.camera_mut(camera)?;
            state.sector = sector;
            state.words[0x3f8 / 4] = sector.is_none().then_some(0);
        }
        Ok(())
    }
    pub(crate) fn destroy_camera_derived(&mut self, key: CameraKey) -> Result<(), ModelError> {
        if let Some(scene) = self.camera(key)?.scene {
            let index = self.scene_index(scene)?;
            self.scenes[index].current_camera = None;
        }
        self.cameras.retain(|camera| camera.key != key);
        Ok(())
    }
    /// Original Scene SetBackdropRange, PC4a840: stores only; it does not
    /// refresh an active camera's existing projection matrices.
    pub fn scene_set_backdrop_range(
        &mut self,
        scene: SceneKey,
        near: f32,
        far: f32,
    ) -> Result<(), ModelError> {
        let index = self.scene_index(scene)?;
        self.scenes[index].backdrop_range = [near, far];
        Ok(())
    }
    pub fn scene_backdrop_range(&self, scene: SceneKey) -> Result<[f32; 2], ModelError> {
        Ok(self.scene(scene)?.backdrop_range)
    }
    /// Descriptive caller projection: Game541030's chunk3200 reads input,
    /// then actual instructions54481d/81f/826 store three zero words.
    pub fn scene_set_zero_field_220(&mut self, scene: SceneKey) -> Result<(), ModelError> {
        let index = self.scene_index(scene)?;
        self.scenes[index].field_220 = [0; 3];
        Ok(())
    }
    pub fn scene_field_220(&self, scene: SceneKey) -> Result<[u32; 3], ModelError> {
        Ok(self.scene(scene)?.field_220)
    }
    pub fn update_frame(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if let Some(sector) = self
            .sectors
            .iter()
            .find(|sector| sector.pending_light_add.is_some())
        {
            return Err(
                crate::scene_ownership::SceneOwnerError::LightAddIncomplete {
                    sector: sector.key,
                    light: sector.pending_light_add.unwrap().light,
                }
                .into(),
            );
        }
        let mut graph = std::mem::take(&mut self.graph);
        let mut payload_error = None;
        let result = graph.update_with_camera(key, |graph, key| {
            if graph.state(key)?.kind == FrameKind::Light {
                graph.update_light_v(key)?;
                let on = graph.state(key)?.flags & 1 != 0;
                let sectors = graph.light_state(key)?.sectors.clone();
                for sector in sectors {
                    let index = self
                        .sectors
                        .iter()
                        .position(|entry| entry.key.frame() == sector)
                        .ok_or(FrameError::UnsupportedFrameUpdate(key))?;
                    if on {
                        self.sectors[index].fog_light = Some(key);
                    } else if self.sectors[index].fog_light == Some(key) {
                        self.sectors[index].fog_light = None;
                        for &candidate in &self.sectors[index].lights {
                            if graph.state(candidate)?.flags & 1 != 0
                                && (5..=7).contains(&graph.light_state(candidate)?.light_type)
                            {
                                self.sectors[index].fog_light = Some(candidate);
                                break;
                            }
                        }
                    }
                }
                return Ok(());
            }
            if graph.state(key)?.kind == FrameKind::Area {
                let area = self
                    .areas
                    .iter_mut()
                    .find(|area| area.key.frame() == key)
                    .ok_or(FrameError::UnsupportedFrameUpdate(key))?;
                return area
                    .update_v(graph, &mut self.area_storage)
                    .map_err(|error| {
                        payload_error = Some(error.into());
                        FrameError::UnsupportedFrameUpdate(key)
                    });
            }
            if graph.state(key)?.kind == FrameKind::Occluder {
                let occluder = self
                    .occluders
                    .iter_mut()
                    .find(|value| value.key.frame() == key)
                    .ok_or(FrameError::UnsupportedFrameUpdate(key))?;
                return crate::frame_occluder::update_occluder_v(
                    graph,
                    occluder,
                    &mut self.occluder_storage,
                )
                .map_err(|_| FrameError::NumericDomain("occluder UpdateV"));
            }
            if graph.state(key)?.kind == FrameKind::Sector {
                return self
                    .sector_update_with_graph(graph, crate::scene_ownership::SectorKey(key))
                    .map_err(|error| {
                        payload_error = Some(error);
                        FrameError::UnsupportedFrameUpdate(key)
                    });
            }
            let index = self
                .cameras
                .iter()
                .position(|camera| camera.key.frame() == key)
                .ok_or(FrameError::UnsupportedFrameUpdate(key))?;
            let mut camera = self.cameras.remove(index);
            std::mem::swap(&mut self.graph, graph);
            let result = update_camera_v(self, &mut camera);
            std::mem::swap(&mut self.graph, graph);
            self.cameras.insert(index, camera);
            result.map_err(|error| {
                payload_error = Some(error);
                FrameError::NumericDomain("camera UpdateV")
            })
        });
        self.graph = graph;
        if let Some(error) = payload_error {
            return Err(error);
        }
        result?;
        Ok(())
    }
    pub fn camera_update(&mut self, key: CameraKey) -> Result<(), ModelError> {
        self.require_camera_ready(key)?;
        self.camera(key)?;
        self.update_frame(key.frame())
    }
}
fn matrix4(a: SMatrix, b: SMatrix) -> SMatrix {
    let mut out = SMatrix { elements: [0.; 16] };
    for row in 0..4 {
        for col in 0..4 {
            let p0 = mul(a.elements[row * 4], b.elements[col]);
            let p1 = mul(a.elements[row * 4 + 1], b.elements[4 + col]);
            let p2 = mul(a.elements[row * 4 + 2], b.elements[8 + col]);
            let p3 = mul(a.elements[row * 4 + 3], b.elements[12 + col]);
            out.elements[row * 4 + col] = add(add(p0, p1), add(p2, p3));
        }
    }
    out
}
fn transform(v: [f32; 3], m: SMatrix) -> [f32; 3] {
    std::array::from_fn(|col| {
        add(
            add(
                add(mul(v[0], m.elements[col]), m.elements[12 + col]),
                mul(v[1], m.elements[4 + col]),
            ),
            mul(v[2], m.elements[8 + col]),
        )
    })
}
fn dot(a: [f32; 3], b: [f32; 3], yzx: bool) -> f32 {
    if yzx {
        add(add(mul(a[1], b[1]), mul(a[2], b[2])), mul(a[0], b[0]))
    } else {
        add(add(mul(a[0], b[0]), mul(a[2], b[2])), mul(a[1], b[1]))
    }
}
fn plane(a: [f32; 3], b: [f32; 3], c: [f32; 3], norm_xzy: bool, dot_yzx: bool) -> ([f32; 3], f32) {
    let ab = std::array::from_fn::<_, 3, _>(|i| sub(a[i], b[i]));
    let cb = std::array::from_fn::<_, 3, _>(|i| sub(c[i], b[i]));
    let mut n = [
        sub(mul(ab[2], cb[1]), mul(ab[1], cb[2])),
        sub(mul(ab[0], cb[2]), mul(cb[0], ab[2])),
        sub(mul(cb[0], ab[1]), mul(ab[0], cb[1])),
    ];
    let squared = if norm_xzy {
        add(add(mul(n[0], n[0]), mul(n[2], n[2])), mul(n[1], n[1]))
    } else {
        dot(n, n, true)
    };
    if f64::from(sub(squared, 1.).abs()) >= 1e-8 {
        if f64::from(squared) >= 1e-8 {
            let r = div(1., sqrt(squared));
            n = n.map(|v| mul(v, r));
        } else {
            let i = if n[0] != 0. {
                0
            } else if n[2] != 0. {
                2
            } else {
                1
            };
            n[i] = if n[i] >= 0. { 1. } else { -1. };
        }
    }
    (n, -dot(n, a, dot_yzx))
}
fn update_camera_v(arena: &mut ModelArena, camera: &mut CameraFrame) -> Result<(), ModelError> {
    if !camera.projection_complete {
        return Err(CameraError::IncompleteProjection(camera.key).into());
    }
    camera.view_incomplete = true;
    if camera.scene.is_none() {
        camera.clear_sector();
    }
    let world = arena
        .graph
        .world_matrix(camera.key.frame())
        .map_err(|_| CameraError::NumericUnavailable("camera world matrix"))?;
    if let Some(scene) = camera.scene {
        let position = world.elements[12..15].try_into().unwrap();
        camera.sector = Some(arena.scene_get_sector(scene, position, camera.sector)?);
        camera.words[0x3f8 / 4] = None;
    }
    let mut view = SMatrix::identity();
    for row in 0..3 {
        for col in 0..3 {
            view.elements[row * 4 + col] = world.elements[col * 4 + row];
        }
    }
    let pos: [f32; 3] = world.elements[12..15].try_into().unwrap();
    let translation = transform(pos.map(|v| -v), view);
    view.elements[12..15].copy_from_slice(&translation);
    camera.put_matrix(0x164, view);
    let (inverse, _) = view
        .inverse()
        .map_err(|_| CameraError::NumericUnavailable("camera view inverse"))?;
    camera.put_matrix(0x224, inverse);
    camera.put_matrix(0x1e4, matrix4(view, camera.matrix_raw(0x1a4).unwrap()));
    let mut bd_view = view;
    bd_view.elements[12..15].fill(0.);
    camera.put_matrix(0x264, bd_view);
    camera.put_matrix(0x324, matrix4(bd_view, camera.matrix_raw(0x2e4).unwrap()));
    let mut bd_inverse = inverse;
    bd_inverse.elements[12..15].fill(0.);
    camera.put_matrix(0x2a4, bd_inverse);
    camera.put_matrix(0x3a4, matrix4(view, camera.matrix_raw(0x364).unwrap()));
    let direction: [f32; 3] = world.elements[8..11].try_into().unwrap();
    for region in 0..3 {
        let base = 0x404 + region * 80;
        for (i, v) in direction.into_iter().enumerate() {
            camera.put(base + i * 4, v);
        }
        camera.put(
            base + 60,
            if region == 1 {
                0.
            } else {
                -dot(direction, pos, true)
            },
        );
    }
    let orthographic = u32::from(camera.orthographic) != 0;
    if orthographic {
        let x = div(1., camera.f(0x160));
        let y = div(x, camera.f(0x158));
        let corners =
            [[-x, y, 0.], [-x, -y, 0.], [x, y, 0.], [x, -y, 0.]].map(|v| transform(v, world));
        let p0 = transform([-x, -y, 1.], world);
        let p1 = transform([x, y, 1.], world);
        for region in 0..2 {
            for (index, (a, b, c)) in [
                (p0, corners[0], corners[1]),
                (p1, corners[3], corners[2]),
                (p1, corners[2], corners[0]),
                (p0, corners[1], corners[3]),
            ]
            .into_iter()
            .enumerate()
            {
                let (n, d) = plane(a, b, c, !(region == 0 && index == 0), false);
                let base = 0x404 + region * 80;
                for (i, v) in n.into_iter().enumerate() {
                    camera.put(base + (index + 1) * 12 + i * 4, v);
                }
                camera.put(base + 60 + (index + 1) * 4, d);
            }
        }
        for i in 0..20 {
            camera.words[(0x4a4 + i * 4) / 4] = camera.words[(0x404 + i * 4) / 4];
        }
    } else {
        for region in 0..3 {
            let (nx, ny, n) = if region == 2 {
                (camera.f(0x3ec), camera.f(0x3f0), camera.f(0x150))
            } else {
                (camera.f(0x3e4), camera.f(0x3e8), camera.f(0x144))
            };
            let x = mul(nx, 100.);
            let y = mul(ny, 100.);
            let z = mul(n, 100.);
            let corners =
                [[-x, y, z], [-x, -y, z], [x, y, z], [x, -y, z]].map(|v| transform(v, world));
            for (index, (b, c)) in [
                (corners[0], corners[1]),
                (corners[3], corners[2]),
                (corners[2], corners[0]),
                (corners[1], corners[3]),
            ]
            .into_iter()
            .enumerate()
            {
                let (n, d) = plane(pos, b, c, region == 2 || (region == 0 && index == 0), true);
                let base = 0x404 + region * 80;
                for (i, v) in n.into_iter().enumerate() {
                    camera.put(base + (index + 1) * 12 + i * 4, v);
                }
                camera.put(base + 60 + (index + 1) * 4, d);
            }
        }
    }
    for offset in (0x164..0x3f4).step_by(4).chain((0x404..0x4f4).step_by(4)) {
        if let Some(bits) = camera.words[offset / 4] {
            let value = f32::from_bits(bits);
            if !value.is_finite() || value != 0. && !value.is_normal() {
                return Err(CameraError::NumericUnavailable("camera view/frustum exponent").into());
            }
        }
    }
    camera.view_incomplete = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver_scene::{
        DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
    };
    use crate::itexture::{
        CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources,
    };
    use crate::native_precision::Pc24PrecisionState;
    use crate::scene_resources::SceneResources;
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
    impl DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            Ok([0, 0, 800, 600])
        }
    }
    struct Images;
    impl TextureImageSource for Images {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            Err(TextureImageError::Missing)
        }
    }
    struct World {
        arena: ModelArena,
        resources: SceneResources,
        textures: TextureResources,
        storage: CpuTextureStorage,
        driver: DriverSceneOwner,
        scene: SceneKey,
    }
    impl World {
        fn new() -> Self {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut Screen,
                    &mut Images,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            Self {
                arena,
                resources,
                textures,
                storage,
                driver,
                scene,
            }
        }
        fn camera(&mut self) -> CameraKey {
            self.driver
                .create_camera_frame(&mut self.arena, &self.resources, self.scene, &mut Screen)
                .unwrap()
        }
        fn close(&mut self) {
            self.resources
                .close_constructed_scene(
                    &mut self.arena,
                    self.scene,
                    &mut self.textures,
                    &mut self.storage,
                )
                .unwrap();
        }
        fn destroy_scene(&mut self) {
            self.driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut self.arena,
                        resources: &mut self.resources,
                        textures: &mut self.textures,
                        storage: &mut self.storage,
                    },
                    self.scene,
                )
                .unwrap();
            self.driver.release_empty_registry_storage().unwrap();
        }
    }
    #[test]
    fn actual_driver_camera_ctor_has_ready_projections_and_unwritten_padding_is_unavailable() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let camera = world.camera();
            let frame = world.arena.camera(camera).unwrap();
            assert_eq!(frame.word(0x140), Some(0x3f490fdb));
            assert_eq!(frame.word(0x15c), None);
            assert_eq!(frame.orthographic_byte(), 0);
            assert_eq!(frame.word(0x404), None);
            assert_eq!(frame.matrix(0x164), Some(SMatrix::identity()));
            let projection = frame.matrix(0x1a4).unwrap();
            assert_eq!(projection.elements[11], 1.);
            assert_eq!(projection.elements[15], 0.);
            assert_ne!(projection, SMatrix::identity());
            assert_eq!(
                world.arena.state(camera.frame()).unwrap().kind,
                FrameKind::Camera
            );
            assert_eq!(world.arena.state(camera.frame()).unwrap().update_count, 1);
            assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 1);
            assert_eq!(world.driver.scenes(), [world.scene]);
            world.arena.release(camera.frame()).unwrap();
            world.destroy_scene();
            assert_eq!(world.arena.live_frame_count(), 0);
        });
    }
    #[test]
    fn close_clears_camera_sector_but_preserves_owning_scene_reference_until_destructor() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let camera = world.camera();
            world
                .arena
                .scene_set_active_camera(world.scene, Some(camera))
                .unwrap();
            assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 2);
            let primary = world.arena.scene(world.scene).unwrap().sectors[0];
            assert_eq!(world.arena.camera(camera).unwrap().sector, Some(primary));
            world
                .arena
                .scene_set_active_camera(world.scene, Some(camera))
                .unwrap();
            assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 2);
            world.arena.release(camera.frame()).unwrap();
            world.close();
            assert_eq!(
                world.arena.scene_active_camera(world.scene).unwrap(),
                Some(camera)
            );
            assert_eq!(world.arena.camera(camera).unwrap().scene, Some(world.scene));
            assert_eq!(world.arena.camera(camera).unwrap().sector, None);
            assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 1);
            world.destroy_scene();
            assert!(world.arena.frame(camera.frame()).is_err());
            assert_eq!(world.arena.live_frame_count(), 0);
        });
    }
    #[test]
    fn external_camera_reference_survives_scene_and_backpointers_are_cleared() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let camera = world.camera();
            world
                .arena
                .scene_set_active_camera(world.scene, Some(camera))
                .unwrap();
            world.destroy_scene();
            assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 1);
            assert_eq!(
                (
                    world.arena.camera(camera).unwrap().scene,
                    world.arena.camera(camera).unwrap().sector
                ),
                (None, None)
            );
            assert_eq!(world.arena.live_frame_count(), 1);
            world.arena.release(camera.frame()).unwrap();
            assert_eq!(world.arena.live_frame_count(), 0);
        });
    }
    #[test]
    fn replacing_or_nulling_active_camera_releases_only_the_scene_reference() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let a = world.camera();
            let b = world.camera();
            world
                .arena
                .scene_set_active_camera(world.scene, Some(a))
                .unwrap();
            world
                .arena
                .scene_set_active_camera(world.scene, Some(b))
                .unwrap();
            assert_eq!(world.arena.frame(a.frame()).unwrap().references, 1);
            assert_eq!(world.arena.camera(a).unwrap().scene, None);
            assert_eq!(world.arena.frame(b.frame()).unwrap().references, 2);
            world
                .arena
                .scene_set_active_camera(world.scene, None)
                .unwrap();
            assert_eq!(world.arena.frame(b.frame()).unwrap().references, 1);
            world.arena.release(a.frame()).unwrap();
            world.arena.release(b.frame()).unwrap();
            world.destroy_scene();
            assert_eq!(world.arena.live_frame_count(), 0);
        });
    }
    #[test]
    fn scene_backdrop_stores_do_not_refresh_projection_until_camera_setter() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let camera = world.camera();
            let previous = world.arena.camera(camera).unwrap().matrix(0x2e4);
            world
                .arena
                .scene_set_active_camera(world.scene, Some(camera))
                .unwrap();
            world
                .arena
                .scene_set_backdrop_range(world.scene, 2., 3000.)
                .unwrap();
            assert_eq!(world.arena.camera(camera).unwrap().matrix(0x2e4), previous);
            world.arena.camera_set_range(camera, 0.05, 250.).unwrap();
            let matrix = world.arena.camera(camera).unwrap().matrix(0x2e4).unwrap();
            assert_eq!(matrix.elements[10], div(3000., sub(3000., 2.)));
            assert_eq!(
                world.arena.scene_field_220(world.scene).unwrap(),
                [0, 0x3f000000, 0x3ecccccd]
            );
            world.arena.scene_set_zero_field_220(world.scene).unwrap();
            assert_eq!(world.arena.scene_field_220(world.scene).unwrap(), [0; 3]);
            world.arena.release(camera.frame()).unwrap();
            world.destroy_scene();
        });
    }
    #[test]
    fn setter_native_ignored_ranges_and_deferred_orthographic_updates_are_distinct() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let camera = world.camera();
            let original = world.arena.camera(camera).unwrap().matrix(0x1a4);
            let count = world.arena.state(camera.frame()).unwrap().update_count;
            world.arena.camera_set_range(camera, 1., 1.).unwrap();
            assert_eq!(
                world.arena.state(camera.frame()).unwrap().update_count,
                count
            );
            assert_eq!(world.arena.camera(camera).unwrap().matrix(0x1a4), original);
            world.arena.camera_set_range(camera, 3., 2.).unwrap();
            assert_eq!(world.arena.camera(camera).unwrap().scalar(0x144), Some(3.));
            world
                .arena
                .camera_set_orthographic_byte(camera, 0x80)
                .unwrap();
            let matrix = world.arena.camera(camera).unwrap().matrix(0x1a4);
            let count = world.arena.state(camera.frame()).unwrap().update_count;
            world.arena.camera_set_aspect_ratio(camera, 2.).unwrap();
            assert_eq!(
                world.arena.state(camera.frame()).unwrap().update_count,
                count
            );
            assert_eq!(world.arena.camera(camera).unwrap().matrix(0x1a4), matrix);
            world.arena.camera_set_orthographic(camera, true).unwrap();
            assert_eq!(
                world
                    .arena
                    .camera(camera)
                    .unwrap()
                    .matrix(0x1a4)
                    .unwrap()
                    .elements[5],
                2.
            );
            world.arena.release(camera.frame()).unwrap();
            world.destroy_scene();
        });
    }
    #[test]
    fn actual_game_camera_sequence_produces_view_and_all_three_frustums_then_closes() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let camera = world.camera();
            world
                .arena
                .scene_set_active_camera(world.scene, Some(camera))
                .unwrap();
            world
                .arena
                .camera_set_direction(camera, [-1., -0.1, -1.], 0.)
                .unwrap();
            world
                .arena
                .camera_set_position(camera, [10., 5., 10.])
                .unwrap();
            world.arena.camera_set_range(camera, 0.05, 250.).unwrap();
            world.arena.camera_update(camera).unwrap();
            world.arena.release(camera.frame()).unwrap();
            world.arena.camera_set_orthographic(camera, false).unwrap();
            world.arena.camera_set_range(camera, 0.05, 250.).unwrap();
            world
                .arena
                .camera_set_fov(camera, f32::from_bits(0x3fc90fdb))
                .unwrap();
            world
                .arena
                .camera_set_aspect_ratio(camera, 800f32 / 600.)
                .unwrap();
            world.arena.camera_update(camera).unwrap();
            let state = world.arena.camera(camera).unwrap();
            assert_ne!(state.matrix(0x164).unwrap(), SMatrix::identity());
            for offset in (0x404..0x4f4).step_by(4) {
                assert!(f32::from_bits(state.word(offset).unwrap()).is_finite());
            }
            world.destroy_scene();
            assert_eq!(world.arena.live_frame_count(), 0);
        });
    }
    #[test]
    fn camera_activation_and_update_select_a_real_loaded_sector_then_primary_outside() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let sector = world
                .driver
                .create_sector_frame(&mut world.arena, &world.resources, world.scene)
                .unwrap();
            let record = lhr_assets::SectorRecord {
                flags: 0x801,
                group: 0,
                vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
                faces: vec![[0, 1, 2]],
                bounds: [[0.; 3], [1., 1., 0.]],
                portals: Vec::new(),
            };
            assert_eq!(
                world.arena.load_sector_record(sector, &record).unwrap(),
                None
            );
            world
                .arena
                .scene_add_frame(world.scene, sector.frame())
                .unwrap();
            world.arena.release(sector.frame()).unwrap();
            let primary = world.arena.scene(world.scene).unwrap().sectors[0];
            world
                .arena
                .link_frame(sector.frame(), Some(primary.frame()), 0)
                .unwrap();
            world.arena.sector_make_ready(sector).unwrap();
            let camera = world.camera();
            world
                .arena
                .scene_set_active_camera(world.scene, Some(camera))
                .unwrap();
            assert_eq!(world.arena.camera(camera).unwrap().sector, Some(sector));
            world.arena.camera_update(camera).unwrap();
            assert_eq!(world.arena.camera(camera).unwrap().sector, Some(sector));
            world
                .arena
                .camera_set_position(camera, [10., 5., 10.])
                .unwrap();
            world.arena.camera_update(camera).unwrap();
            assert_eq!(world.arena.camera(camera).unwrap().sector, Some(primary));
            world.arena.release(camera.frame()).unwrap();
            world.destroy_scene();
            assert_eq!(world.arena.live_frame_count(), 0);
            assert_eq!(world.arena.sector_storage().live_allocations(), 0);
        });
    }
    #[test]
    fn foreign_camera_key_does_not_alias_same_numeric_frame_in_another_arena() {
        Pc24PrecisionState::default().run(|| {
            let mut a = World::new();
            let mut b = World::new();
            let ca = a.camera();
            let cb = b.camera();
            assert_eq!(ca.frame(), cb.frame());
            a.arena.scene_set_active_camera(a.scene, Some(ca)).unwrap();
            assert!(matches!(
                a.arena.scene_set_active_camera(a.scene, Some(cb)),
                Err(ModelError::Camera(CameraError::ForeignArena))
            ));
            assert_eq!(a.arena.frame(ca.frame()).unwrap().references, 2);
            a.arena.release(ca.frame()).unwrap();
            b.arena.release(cb.frame()).unwrap();
            a.destroy_scene();
            b.destroy_scene();
        });
    }
    #[test]
    fn actual_constructor_sector_update_runs_base_child_effects_without_invented_geometry() {
        Pc24PrecisionState::default().run(|| {
            let mut world = World::new();
            let sector = world.arena.scene(world.scene).unwrap().sectors[0];
            world
                .arena
                .graph
                .change_flags(sector.frame(), 0, 0x140)
                .unwrap();
            world.arena.update_frame(sector.frame()).unwrap();
            assert_eq!(
                world.arena.state(sector.frame()).unwrap().flags & 0x140,
                0x140
            );
            assert_eq!(world.arena.state(sector.frame()).unwrap().update_count, 1);
            let child = world
                .arena
                .insert(I3dFrameState::dummy(), None, Vec::new(), Vec::new(), None)
                .unwrap();
            world.arena.scene_add_frame(world.scene, child).unwrap();
            world
                .arena
                .link_frame(child, Some(sector.frame()), 0)
                .unwrap();
            world.arena.release(child).unwrap();
            world.arena.update_frame(sector.frame()).unwrap();
            assert_eq!(world.arena.state(child).unwrap().update_count, 1);
            assert_eq!(world.arena.state(child).unwrap().flags & 0x40002000, 0);
            // Native UpdateChildren can invalidate the parent's HR flags even
            // though receiver-only DiscardHRBoundVol skips the sector.
            assert_eq!(world.arena.state(sector.frame()).unwrap().flags & 0x140, 0);
            world
                .arena
                .sectors
                .iter_mut()
                .find(|entry| entry.key == sector)
                .unwrap()
                .sector_flags |= 4;
            world.arena.update_frame(sector.frame()).unwrap();
            assert_eq!(world.arena.sector(sector).unwrap().geometry.plane_count, 0);
            assert!(world
                .arena
                .sector(sector)
                .unwrap()
                .geometry
                .portals
                .is_empty());
            world
                .arena
                .sectors
                .iter_mut()
                .find(|entry| entry.key == sector)
                .unwrap()
                .sector_flags &= !4;
            world.destroy_scene();
        });
    }
    #[test]
    fn native_snan_store_prefix_quiets_only_fpu_stores_and_new_camera_defaults_survive() {
        Pc24PrecisionState::default().run(|| {
            // Exact stored words from actual original setters, O0/O3 native
            // execution. Projection on nonfinite input remains unavailable.
            for (operation, offset, expected) in [
                (0, 0x158, 0x7fc0_0123),
                (1, 0x144, 0xffc0_0456),
                (2, 0x148, 0x7f80_0123),
                (3, 0x140, 0xff80_0456),
                (4, 0x150, 0xffc0_0456),
                (5, 0x160, 0x7f80_0123),
            ] {
                let mut world = World::new();
                let camera = world.camera();
                world
                    .arena
                    .scene_set_active_camera(world.scene, Some(camera))
                    .unwrap();
                let result = match operation {
                    0 => world
                        .arena
                        .camera_set_aspect_ratio(camera, f32::from_bits(0x7f80_0123)),
                    1 => world
                        .arena
                        .camera_set_range(camera, f32::from_bits(0xff80_0456), 250.),
                    2 => world
                        .arena
                        .camera_set_range(camera, 0.05, f32::from_bits(0x7f80_0123)),
                    3 => world
                        .arena
                        .camera_set_fov(camera, f32::from_bits(0xff80_0456)),
                    4 => {
                        world
                            .arena
                            .camera_set_near_range(camera, f32::from_bits(0xff80_0456), 20.)
                    }
                    _ => world
                        .arena
                        .camera_set_ortho_scale(camera, f32::from_bits(0x7f80_0123)),
                };
                if operation == 5 {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(
                        result,
                        Err(ModelError::Camera(CameraError::NumericUnavailable(_)))
                    ));
                }
                assert_eq!(
                    world.arena.camera(camera).unwrap().word(offset),
                    Some(expected)
                );
                assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 2);
                if operation != 5 {
                    assert_eq!(world.arena.camera(camera).unwrap().matrix(0x1a4), None);
                }
                let fresh = world.camera();
                assert_eq!(
                    world.arena.camera(fresh).unwrap().word(0x140),
                    Some(0x3f49_0fdb)
                );
                assert_eq!(
                    world.arena.camera(fresh).unwrap().word(0x144),
                    Some(0x3f80_0000)
                );
                assert_eq!(
                    world.arena.camera(fresh).unwrap().word(0x148),
                    Some(0x447a_0000)
                );
                assert!(world.arena.camera(fresh).unwrap().projection_complete());
                world.arena.release(fresh.frame()).unwrap();
                world.arena.release(camera.frame()).unwrap();
                world.destroy_scene();
                assert_eq!(world.arena.live_frame_count(), 0);
            }
        });
    }
    #[test]
    fn native_ctor_integer_height_ratio_avoids_double_rounding_and_releases_camera() {
        struct RatioScreen {
            height: i32,
            calls: Vec<&'static str>,
        }
        impl SceneScreen for RatioScreen {
            type Error = ();
            fn screen_sx(&mut self) -> Result<i32, ()> {
                self.calls.push("sx");
                Ok(953_505_280)
            }
            fn screen_sy(&mut self) -> Result<i32, ()> {
                self.calls.push("sy");
                Ok(self.height)
            }
        }
        // Original driver3 -> complete camera ctor/projection/dtor, O0/O3.
        // The counter ratio lies strictly below the binary32 midpoint while
        // binary64 division lands on it and would incorrectly round upward.
        for (height, aspect, projected_y, ncp_y, near_ncp_y) in [
            (
                1_907_010_219,
                0x3f00_0001,
                0x3f9a_827a,
                0x3f54_13cc,
                0x3c07_bac0,
            ),
            (
                1_907_010_218,
                0x3f00_0002,
                0x3f9a_827b,
                0x3f54_13cb,
                0x3c07_babf,
            ),
            (
                1_907_010_220,
                0x3f00_0001,
                0x3f9a_827a,
                0x3f54_13cc,
                0x3c07_bac0,
            ),
            (
                -1_907_010_219,
                0xbf00_0001,
                0xbf9a_827a,
                0xbf54_13cc,
                0xbc07_bac0,
            ),
        ] {
            Pc24PrecisionState::default().run(|| {
                let mut world = World::new();
                let mut screen = RatioScreen {
                    height,
                    calls: Vec::new(),
                };
                let camera = world
                    .driver
                    .create_camera_frame(
                        &mut world.arena,
                        &world.resources,
                        world.scene,
                        &mut screen,
                    )
                    .unwrap();
                assert_eq!(screen.calls, ["sx", "sy"]);
                let state = world.arena.camera(camera).unwrap();
                assert_eq!(state.word(0x158), Some(aspect));
                assert_eq!(
                    state.matrix(0x1a4).unwrap().elements.map(f32::to_bits),
                    [
                        0x401a_8279,
                        0,
                        0,
                        0,
                        0,
                        projected_y,
                        0,
                        0,
                        0,
                        0,
                        0x3f80_20cd,
                        0x3f80_0000,
                        0,
                        0,
                        0xbf80_20cd,
                        0,
                    ]
                );
                assert_eq!(state.word(0x3e8), Some(ncp_y));
                assert_eq!(state.word(0x3f0), Some(near_ncp_y));
                assert_eq!(world.arena.frame(camera.frame()).unwrap().references, 1);
                world.arena.release(camera.frame()).unwrap();
                assert!(world.arena.frame(camera.frame()).is_err());
                world.destroy_scene();
                assert_eq!(world.arena.live_frame_count(), 0);
            });
        }
        assert_eq!(
            ((953_505_280f64 / 1_907_010_219f64) as f32).to_bits(),
            0x3f00_0002
        );
    }
}
