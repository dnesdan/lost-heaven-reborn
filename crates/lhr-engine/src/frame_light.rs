//! CPU Light constructor, local setters and unregistered light UpdateV.
//! Actual PS2 I3D_light symbols corroborate SetLightType, SetDiffuse,
//! SetDiffusePower, SetRange, SetCone, UpdateV and SetMode; PC layouts govern.
//! Original PC entries:201f0,20760,20800,20930,209e0,20a90,20b20,20c30,20d10,21400,21440,21490,21530.
//! Sector registration shares the ModelArena's sector lists and the graph's
//! reciprocal light lists. Rendering and fog-derived light types remain separate.
use crate::{
    frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameGraph, I3dFrameState},
    model_loading::{ModelArena, ModelError},
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::{
        native_trig::{light_cone_cosines, light_cone_radius},
        SQuat,
    },
};

#[derive(Clone, Debug)]
pub struct I3dLightState {
    pub(crate) constructor_complete: bool,
    fault: Option<FrameError>,
    /// Host AddLight interruption marker; UpdateV cannot cross this prefix.
    pub(crate) pending_sector_add: bool,
    pub mode: u32,
    pub light_type: u32,
    pub intensity: f32,
    pub color: [f32; 3],
    pub range: [f32; 2],
    pub cone: [f32; 2],
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub scaled_color: [f32; 3],
    pub near_scaled: f32,
    pub outer_cos: f32,
    pub inner_cos: f32,
    pub cone_reciprocal: f32,
    pub far_scaled: f32,
    pub range_difference: f32,
    pub far_squared: f32,
    pub cone_radius: f32,
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
    pub sectors: Vec<FrameKey>,
}
impl I3dLightState {
    pub(crate) fn constructor_prefix() -> Self {
        Self {
            constructor_complete: false,
            fault: None,
            pending_sector_add: false,
            mode: 0xff6b,
            light_type: 3,
            intensity: 1.,
            color: [1.; 3],
            range: [1., 100.],
            cone: [f32::from_bits(0x3eb2b8c3), f32::from_bits(0x3f32b8c3)],
            position: [0.; 3],
            direction: [0.; 3],
            scaled_color: [1.; 3],
            near_scaled: 0.,
            outer_cos: 0.,
            inner_cos: 0.,
            cone_reciprocal: 0.,
            far_scaled: 0.,
            range_difference: 0.,
            far_squared: 0.,
            cone_radius: 0.,
            sphere_center: [0.; 3],
            sphere_radius: 0.,
            sectors: Vec::new(),
        }
    }
}
/// Descriptive owner for the original CRT rand stream used by light creation.
/// It is explicit state, not a fixed random result or system RNG substitution.
#[derive(Debug)]
pub struct LightCrtRandom {
    state: u32,
}
impl Default for LightCrtRandom {
    fn default() -> Self {
        Self { state: 1 }
    }
}
impl LightCrtRandom {
    pub fn seed(&mut self, state: u32) {
        self.state = state;
    }
    pub fn state(&self) -> u32 {
        self.state
    }
    pub(crate) fn next_value(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
        (self.state >> 16) & 0x7fff
    }
}
/// Fog-kind set shared by the sector owner (AddLight/DeleteLight/UpdateV
/// fog arms, PC RVAs 0x4f3d0/0x4f550/0x20d10): FOG/POINTAMBIENT/POINTFOG.
pub(crate) fn light_is_fog_kind(kind: u32) -> bool {
    matches!(kind, 5..=7)
}
fn numeric(_: crate::native_matrix::MatrixArithmeticUnavailable) -> FrameError {
    FrameError::NumericDomain("Light")
}
fn mul(a: f32, b: f32) -> Result<f32, FrameError> {
    checked_f32_mul(a, b).map_err(numeric)
}
fn add(a: f32, b: f32) -> Result<f32, FrameError> {
    checked_f32_add(a, b).map_err(numeric)
}
fn sub(a: f32, b: f32) -> Result<f32, FrameError> {
    // Keep subtraction as one native PC24 operation.
    let v = (a as f64) - (b as f64);
    let f = v as f32;
    if !a.is_finite()
        || !b.is_finite()
        || !f.is_finite()
        || (f != 0. && !f.is_normal())
        || (f == 0. && v != 0.)
    {
        return Err(FrameError::NumericDomain("Light"));
    }
    Ok(f)
}
fn normalize(mut v: [f32; 3]) -> Result<[f32; 3], FrameError> {
    let squared = add(add(mul(v[1], v[1])?, mul(v[2], v[2])?)?, mul(v[0], v[0])?)?;
    if f64::from(sub(squared, 1.)?.abs()) >= 1e-8 {
        if squared >= 1e-8 {
            let reciprocal = 1. / squared.sqrt();
            for x in &mut v {
                *x = mul(*x, reciprocal)?;
            }
        } else {
            let index = if v[0] != 0. {
                0
            } else if v[2] != 0. {
                2
            } else {
                1
            };
            v[index] = if v[index] < 0. { -1. } else { 1. };
        }
    }
    Ok(v)
}
impl I3dFrameGraph {
    fn dirty_light_sectors(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let count = self.light_state(key)?.sectors.len();
        for index in (0..count).rev() {
            let sector = self.light_state(key)?.sectors[index];
            let changed = self.sector_light_changes.entry(sector.0).or_default();
            *changed = changed.wrapping_add(1);
        }
        Ok(())
    }
    fn light_scaled_range(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let scale = self.state(key)?.world_scale;
        let range = self.light_state(key)?.range;
        let near = mul(range[0], scale)?;
        self.light_state_mut(key)?.near_scaled = near;
        let far = mul(range[1], scale)?;
        self.light_state_mut(key)?.far_scaled = far;
        self.light_state_mut(key)?.far_squared = mul(far, far)?;
        self.light_state_mut(key)?.range_difference = sub(far, near)?;
        Ok(())
    }
    fn light_percent_range(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let range = self.light_state(key)?.range;
        let near = mul(range[0], 0.01)?;
        self.light_state_mut(key)?.near_scaled = near;
        let far = mul(range[1], 0.01)?;
        self.light_state_mut(key)?.far_scaled = far;
        Ok(())
    }
    fn light_cone_cache(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let light = self.light_state(key)?;
        let values = light_cone_cosines(light.cone[0], light.cone[1])
            .map_err(|_| FrameError::NumericDomain("Light cone"))?;
        let light = self.light_state_mut(key)?;
        light.outer_cos = values[0];
        light.inner_cos = values[1];
        light.cone_reciprocal = values[2];
        Ok(())
    }
    fn light_sphere(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let radius = mul(
            self.light_state(key)?.cone_radius,
            self.state(key)?.world_scale,
        )?;
        self.light_state_mut(key)?.sphere_radius = radius;
        let world = self.world_matrix(key)?;
        let direction = normalize(world.elements[8..11].try_into().unwrap())?;
        let p = [
            mul(direction[0], radius)?,
            mul(direction[1], radius)?,
            mul(direction[2], radius)?,
        ];
        let offsets = p;
        self.light_state_mut(key)?.sphere_center = [
            add(offsets[0], world.elements[12])?,
            add(offsets[1], world.elements[13])?,
            add(offsets[2], world.elements[14])?,
        ];
        Ok(())
    }
    fn update_light_v_inner(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if !self.light_state(key)?.constructor_complete {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        self.projected_state_mut(key)?.update_count = self.state(key)?.update_count.wrapping_add(1);
        self.dirty_light_sectors(key)?;
        match self.light_state(key)?.light_type {
            1 => {
                let world = self.world_matrix(key)?;
                self.light_state_mut(key)?.position = world.elements[12..15].try_into().unwrap();
                self.light_scaled_range(key)?;
            }
            2 => {
                let world = self.world_matrix(key)?;
                self.light_state_mut(key)?.position = world.elements[12..15].try_into().unwrap();
                self.light_state_mut(key)?.direction =
                    normalize(world.elements[8..11].try_into().unwrap())?;
                self.light_scaled_range(key)?;
                self.light_cone_cache(key)?;
                self.light_sphere(key)?;
            }
            3 => {
                let world = self.world_matrix(key)?;
                self.light_state_mut(key)?.direction =
                    normalize(world.elements[8..11].try_into().unwrap())?;
            }
            4 => {}
            // Fog, point ambient and point fog use unscaled scene units here.
            // Their active-sector selection belongs to the sector owner.
            5..=7 => self.light_percent_range(key)?,
            _ => return Err(FrameError::UnsupportedFrameUpdate(key)),
        }
        Ok(())
    }
    fn set_light_type_inner(&mut self, key: FrameKey, kind: u32) -> Result<(), FrameError> {
        let old = self.light_state(key)?.light_type;
        if old == kind {
            return Ok(());
        }
        // Original linked fog<->fog transition runs the sector Delete/Add
        // path (0x20800 body): counts and selection are the sector owner's
        // effects, untouched by this graph-only store, as is the second
        // lights_changed bump (native Delete+Add bumps twice; this store
        // keeps the single dirty bump — disclosed). With >=2 fog lights
        // native ends with the re-added light selected while this store keeps
        // the prior selection (disclosed edge); single-fog states coincide.
        // Only fog<->non-fog needs the sector owner to reorder first.
        // Callers of this graph-only setter have no sector owner to perform
        // that effect. Scene2 unlinks before SetLightType.
        if !(1..=7).contains(&kind)
            || (!self.light_state(key)?.sectors.is_empty()
                && (light_is_fog_kind(old) != light_is_fog_kind(kind)))
        {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        self.projected_state_mut(key)?.update_count = self.state(key)?.update_count.wrapping_add(1);
        self.dirty_light_sectors(key)?;
        self.light_state_mut(key)?.light_type = kind;
        if kind == 2 {
            let l = self.light_state(key)?;
            let radius = light_cone_radius(l.cone[1], l.range[1])
                .map_err(|_| FrameError::NumericDomain("Light cone"))?;
            self.light_state_mut(key)?.cone_radius = radius;
        }
        Ok(())
    }
    fn set_light_mode_inner(&mut self, key: FrameKey, mode: u32) -> Result<(), FrameError> {
        if self.light_state(key)?.mode != mode {
            self.light_state_mut(key)?.mode = mode;
            self.projected_state_mut(key)?.update_count =
                self.state(key)?.update_count.wrapping_add(1);
            self.dirty_light_sectors(key)?;
        }
        Ok(())
    }
    fn light_color_cache(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let l = self.light_state(key)?;
        let color = [
            mul(l.color[0], l.intensity)?,
            mul(l.color[1], l.intensity)?,
            mul(l.color[2], l.intensity)?,
        ];
        self.light_state_mut(key)?.scaled_color = color;
        self.projected_state_mut(key)?.update_count = self.state(key)?.update_count.wrapping_add(1);
        self.dirty_light_sectors(key)?;
        Ok(())
    }
    fn set_light_diffuse_inner(
        &mut self,
        key: FrameKey,
        color: [f32; 3],
    ) -> Result<(), FrameError> {
        self.light_state_mut(key)?.color = color;
        self.light_color_cache(key)
    }
    fn set_light_diffuse_power_inner(
        &mut self,
        key: FrameKey,
        intensity: f32,
    ) -> Result<(), FrameError> {
        self.light_state_mut(key)?.intensity = intensity;
        self.light_color_cache(key)
    }
    fn set_light_range_inner(
        &mut self,
        key: FrameKey,
        near: f32,
        far: f32,
    ) -> Result<(), FrameError> {
        self.light_state_mut(key)?.range = [near, if far > near { far } else { near }];
        if (5..=7).contains(&self.light_state(key)?.light_type) {
            self.light_percent_range(key)?;
        } else {
            self.light_scaled_range(key)?;
        }
        self.projected_state_mut(key)?.update_count = self.state(key)?.update_count.wrapping_add(1);
        self.dirty_light_sectors(key)?;
        if self.light_state(key)?.light_type == 2 {
            let l = self.light_state(key)?;
            let radius = light_cone_radius(l.cone[1], l.range[1])
                .map_err(|_| FrameError::NumericDomain("Light cone"))?;
            self.light_state_mut(key)?.cone_radius = radius;
        }
        Ok(())
    }
    fn set_light_cone_inner(
        &mut self,
        key: FrameKey,
        inner: f32,
        outer: f32,
    ) -> Result<(), FrameError> {
        self.light_state_mut(key)?.cone = [inner, outer];
        self.light_cone_cache(key)?;
        self.projected_state_mut(key)?.update_count = self.state(key)?.update_count.wrapping_add(1);
        self.dirty_light_sectors(key)?;
        if self.light_state(key)?.light_type == 2 {
            let l = self.light_state(key)?;
            let radius = light_cone_radius(l.cone[1], l.range[1])
                .map_err(|_| FrameError::NumericDomain("Light cone"))?;
            self.light_state_mut(key)?.cone_radius = radius;
        }
        Ok(())
    }
}
impl I3dFrameGraph {
    fn light_effect(
        &mut self,
        key: FrameKey,
        effect: impl FnOnce(&mut Self) -> Result<(), FrameError>,
    ) -> Result<(), FrameError> {
        if let Some(error) = self.light_state(key)?.fault {
            return Err(error);
        }
        let result = effect(self);
        if let Err(error) = result {
            self.light_state_mut(key)?.fault = Some(error);
        }
        result
    }
    pub(crate) fn update_light_v(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.update_light_v_inner(key))
    }
    pub fn set_light_type(&mut self, key: FrameKey, kind: u32) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.set_light_type_inner(key, kind))
    }
    pub fn set_light_mode(&mut self, key: FrameKey, mode: u32) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.set_light_mode_inner(key, mode))
    }
    pub fn set_light_diffuse(&mut self, key: FrameKey, color: [f32; 3]) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.set_light_diffuse_inner(key, color))
    }
    pub fn set_light_diffuse_power(&mut self, key: FrameKey, power: f32) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.set_light_diffuse_power_inner(key, power))
    }
    pub fn set_light_range(
        &mut self,
        key: FrameKey,
        near: f32,
        far: f32,
    ) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.set_light_range_inner(key, near, far))
    }
    pub fn set_light_cone(
        &mut self,
        key: FrameKey,
        inner: f32,
        outer: f32,
    ) -> Result<(), FrameError> {
        self.light_effect(key, |graph| graph.set_light_cone_inner(key, inner, outer))
    }
}
impl I3dFrameGraph {
    /// PC LoadImage writes mode through cone_out after common-frame intake.
    /// Keep constructor caches and counters intact; the subsequent Update path
    /// owns recomputation, and malformed numerical domains fail there.
    pub(crate) fn apply_light_load_image_fields(
        &mut self,
        key: FrameKey,
        record: &lhr_assets::LightRecord,
    ) -> Result<(), FrameError> {
        let light = self.light_state_mut(key)?;
        light.mode = record.mode;
        light.light_type = record.light_type;
        light.intensity = record.power;
        light.color = record.diffuse;
        light.range = record.range;
        light.cone = record.cone;
        Ok(())
    }
    /// LoadImage intake guard, PC RVA 0x21530: the frame intake runs before
    /// the 0x28 dtaRead. An intake failure records the Light LoadImage fault
    /// marker and returns the inner error prefix (reached error, never a
    /// bare frame error); caches stay untouched (no pollution residue).
    pub(crate) fn apply_light_load_image_checked(
        &mut self,
        key: FrameKey,
        record: &lhr_assets::LightRecord,
        intake: Result<(), FrameError>,
    ) -> Result<(), FrameError> {
        if let Err(error) = intake {
            self.light_state_mut(key)?.fault = Some(FrameError::NumericDomain("Light LoadImage"));
            return Err(error);
        }
        self.apply_light_load_image_fields(key, record)
    }
}
impl ModelArena {
    pub fn light(&self, key: FrameKey) -> Result<&I3dLightState, ModelError> {
        let state = self.graph.light_state(key)?;
        if let Some(error) = state.fault {
            return Err(error.into());
        }
        if !state.constructor_complete {
            return Err(FrameError::UnsupportedFrameUpdate(key).into());
        }
        Ok(state)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the light-type member (I3D.h:1931 `I3DLIGHTTYPE ltype`; mafia-re
    /// ls3df/I3D/I3D.h at pin 113b5b6). The original exposes no named getter:
    /// the light virtuals are setters plus UpdateV (I3D.h:1915-1926), and
    /// callers read the member in place. Same availability guard as `light`.
    pub fn light_type_of(&self, key: FrameKey) -> Result<u32, ModelError> {
        Ok(self.light(key)?.light_type)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the light-mode member (I3D.h:1930 `uint mode, I3DLIGHTMODE_*`;
    /// mafia-re ls3df/I3D/I3D.h at pin 113b5b6). No named original getter;
    /// same availability guard as `light`.
    pub fn light_mode_of(&self, key: FrameKey) -> Result<u32, ModelError> {
        Ok(self.light(key)?.mode)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the diffuse store (I3D.h:1933 `S_vector diffuse`; SetDiffuse writes
    /// `color`, which this projection keeps as the diffuse word triple).
    /// No named original getter; same availability guard as `light`.
    pub fn light_diffuse_of(&self, key: FrameKey) -> Result<[f32; 3], ModelError> {
        Ok(self.light(key)?.color)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the power store (I3D.h:1932 `float power`; SetDiffusePower writes
    /// `intensity`). No named original getter; same availability guard as
    /// `light`.
    pub fn light_power_of(&self, key: FrameKey) -> Result<f32, ModelError> {
        Ok(self.light(key)?.intensity)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the range store (I3D.h:1934 `float near_range, far_range`; SetRange
    /// writes `range`). No named original getter; same availability guard as
    /// `light`.
    pub fn light_range_of(&self, key: FrameKey) -> Result<[f32; 2], ModelError> {
        Ok(self.light(key)?.range)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the cone store (I3D.h:1936 `float cone_in, cone_out`; SetCone writes
    /// `cone`). No named original getter; same availability guard as `light`.
    pub fn light_cone_of(&self, key: FrameKey) -> Result<[f32; 2], ModelError> {
        Ok(self.light(key)?.cone)
    }
    pub fn light_random_state(&mut self) -> &mut LightCrtRandom {
        &mut self.light_random
    }
    pub fn create_light(&mut self) -> Result<FrameKey, ModelError> {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Light;
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        self.finish_light_constructor(key)
    }
    /// Original SetOn, PC RVA 0x21400 (contract: entry MOV AL/MOV ECX + JZ
    /// mid, no Ghidra body; mafia-re I3d_light.cpp:241-253). Upstream working
    /// name, not an evidenced original symbol. Idempotent FRMFLAGS_ON test
    /// first (no count bump when unchanged); on change the UpdateV tail owns
    /// caches, then the sector fog reselect runs for fog kinds (native
    /// UpdateV inline order, split across the graph/sector owners here).
    pub fn set_light_on(&mut self, key: FrameKey, on: bool) -> Result<(), ModelError> {
        let is_on = self.state(key)?.flags & 1 != 0;
        if on == is_on {
            return Ok(());
        }
        if on {
            self.graph.change_flags(key, 0, 1)?;
        } else {
            self.graph.change_flags(key, 1, 0)?;
        }
        self.graph.update_light_v_inner(key)?;
        if light_is_fog_kind(self.graph.light_state(key)?.light_type) {
            self.light_update_fog_selection(key, on)?;
        }
        Ok(())
    }
    /// Original Duplicate, PC RVA 0x21490 (contract: entry PUSH ESI/EDI +
    /// TEST null-check shape, no Ghidra body; mafia-re I3d_light.cpp:269-286).
    /// Upstream working name, not an evidenced original symbol. Effect order:
    /// base duplicate_frame first, cross-type early OK (base only), field
    /// copy (diffuse/type/power/near/far/cone_in/cone_out — never caches,
    /// sectors or mode), UpdateV tail rebuilds cosines/scaled ranges only
    /// (cone/sphere radii and scaled_color stay dst-construction values: no
    /// UpdateV arm recomputes them — native-stale, disclosed). Self-duplicate
    /// flows through the same path as a benign no-op (documented; no invented
    /// aliasing error).
    pub fn duplicate_light(&mut self, dst: FrameKey, src: FrameKey) -> Result<(), ModelError> {
        self.duplicate_frame(dst, src)?;
        if self.state(dst)?.kind != FrameKind::Light || self.state(src)?.kind != FrameKind::Light {
            return Ok(());
        }
        let (color, light_type, intensity, range, cone) = {
            let s = self.graph.light_state(src)?;
            (s.color, s.light_type, s.intensity, s.range, s.cone)
        };
        let d = self.graph.light_state_mut(dst)?;
        d.color = color;
        d.light_type = light_type;
        d.intensity = intensity;
        d.range = range;
        d.cone = cone;
        self.graph.update_light_v_inner(dst)?;
        Ok(())
    }
    pub(crate) fn finish_light_constructor(
        &mut self,
        key: FrameKey,
    ) -> Result<FrameKey, ModelError> {
        let result: Result<FrameKey, ModelError> = (|| {
            self.graph.begin_light_constructor(key)?;
            let rotation = SQuat::try_make_frame_direction([1., -1., 1.], 0.)
                .map_err(|_| ModelError::Frame(FrameError::NumericDomain("Light SetDir")))?;
            let state = self.graph.projected_state_mut(key)?;
            state.rotation = rotation;
            state.flags = (state.flags & 0xfffffecb) | 0x40000008;
            state.flags |= 0x10;
            state.rotation.normalize();
            state
                .local
                .try_set_rot3(state.rotation)
                .map_err(|_| ModelError::Frame(FrameError::NumericDomain("Light SetRot3")))?;
            let local_direction = state.local.elements[8..11].try_into().unwrap();
            self.graph.light_state_mut(key)?.direction = local_direction;
            self.graph.light_scaled_range(key)?;
            self.graph.light_cone_cache(key)?;
            self.graph.projected_state_mut(key)?.update_count = self.light_random.next_value();
            let l = self.graph.light_state(key)?;
            let radius = light_cone_radius(l.cone[1], l.range[1])
                .map_err(|_| FrameError::NumericDomain("Light cone"))?;
            self.graph.light_state_mut(key)?.cone_radius = radius;
            self.graph.light_sphere(key)?;
            self.graph.light_state_mut(key)?.constructor_complete = true;
            if let Err(error) = self.graph.update(key) {
                self.graph.light_state_mut(key)?.constructor_complete = false;
                return Err(error.into());
            }
            Ok(key)
        })();
        result.map_err(|cause| ModelError::LightConstructorUnavailable {
            frame: key,
            cause: Box::new(cause),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn load_image_raw_write_keeps_constructor_caches_and_counter() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        let before = arena.light(key).unwrap().clone();
        let count = arena.state(key).unwrap().update_count;
        arena
            .graph
            .apply_light_load_image_fields(
                key,
                &lhr_assets::LightRecord {
                    mode: 0x87654321,
                    light_type: u32::MAX,
                    power: f32::from_bits(0x7f801234),
                    diffuse: [-0., 0.25, 0.5],
                    range: [12., 45.],
                    cone: [0.2, 0.8],
                },
            )
            .unwrap();
        let after = arena.light(key).unwrap();
        assert_eq!(after.intensity.to_bits(), 0x7f801234);
        assert_eq!(after.color[0].to_bits(), 0x80000000);
        assert_eq!(after.light_type, u32::MAX);
        assert_eq!(after.mode, 0x87654321);
        assert_eq!(after.scaled_color, before.scaled_color);
        assert_eq!(after.near_scaled, before.near_scaled);
        assert_eq!(after.far_scaled, before.far_scaled);
        assert_eq!(after.cone_radius, before.cone_radius);
        assert_eq!(after.sphere_center, before.sphere_center);
        assert_eq!(after.cone_reciprocal, before.cone_reciprocal);
        assert_eq!(arena.state(key).unwrap().update_count, count);
        // Unsupported light kinds stay explicit when runtime Update is requested.
        assert!(arena.graph.update(key).is_err());
    }
    #[test]
    fn light_type_reader_reports_constructor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        assert_eq!(arena.light_type_of(key).unwrap(), 3);
        arena.graph.set_light_type(key, 2).unwrap();
        assert_eq!(arena.light_type_of(key).unwrap(), 2);
    }
    #[test]
    fn light_mode_reader_reports_constructor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        assert_eq!(arena.light_mode_of(key).unwrap(), 0xff6b);
        arena.graph.set_light_mode(key, 0x21).unwrap();
        assert_eq!(arena.light_mode_of(key).unwrap(), 0x21);
    }
    #[test]
    fn light_diffuse_reader_reports_constructor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        assert_eq!(arena.light_diffuse_of(key).unwrap(), [1.; 3]);
        arena.graph.set_light_diffuse(key, [0.25, 0.5, 0.75]).unwrap();
        assert_eq!(arena.light_diffuse_of(key).unwrap(), [0.25, 0.5, 0.75]);
    }
    #[test]
    fn light_power_reader_reports_constructor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        assert_eq!(arena.light_power_of(key).unwrap(), 1.);
        arena.graph.set_light_diffuse_power(key, 2.).unwrap();
        assert_eq!(arena.light_power_of(key).unwrap(), 2.);
    }
    #[test]
    fn light_range_reader_reports_constructor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        assert_eq!(arena.light_range_of(key).unwrap(), [1., 100.]);
        arena.graph.set_light_range(key, 5., 50.).unwrap();
        assert_eq!(arena.light_range_of(key).unwrap(), [5., 50.]);
    }
    #[test]
    fn light_cone_reader_reports_constructor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        assert_eq!(
            arena.light_cone_of(key).unwrap(),
            [f32::from_bits(0x3eb2b8c3), f32::from_bits(0x3f32b8c3)]
        );
        arena.graph.set_light_cone(key, 0.3, 0.9).unwrap();
        assert_eq!(arena.light_cone_of(key).unwrap(), [0.3, 0.9]);
    }
    #[test]
    fn interrupted_numeric_effect_never_becomes_a_valid_light_cache() {
        let mut arena = ModelArena::default();
        let light = arena.create_light().unwrap();
        assert!(arena.graph.set_light_diffuse_power(light, f32::MAX).is_ok());
        assert!(arena.graph.set_light_diffuse(light, [2., 1., 1.]).is_err());
        assert_eq!(arena.graph.light_state(light).unwrap().color, [2., 1., 1.]);
        assert!(arena.light(light).is_err());
        assert!(arena.graph.update(light).is_err());
        arena.release(light).unwrap();
        assert!(arena.frame(light).is_err());
    }
    #[test]
    fn fog_percent_range_retains_near_store_when_far_arithmetic_fails() {
        let mut arena = ModelArena::default();
        let light = arena.create_light().unwrap();
        arena.graph.set_light_type(light, 5).unwrap();
        let old_far = arena.graph.light_state(light).unwrap().far_scaled;
        assert!(arena
            .graph
            .set_light_range(light, 20., f32::INFINITY)
            .is_err());
        let prefix = arena.graph.light_state(light).unwrap();
        assert_eq!(prefix.range, [20., f32::INFINITY]);
        assert_eq!(prefix.near_scaled.to_bits(), (20f32 * 0.01).to_bits());
        assert_eq!(prefix.far_scaled.to_bits(), old_far.to_bits());
        assert!(arena.light(light).is_err());
        arena.release(light).unwrap();
    }
    #[test]
    fn constructor_and_spot_setters_compose_live_graph_and_last_release() {
        let mut arena = ModelArena::default();
        arena.light_random_state().seed(1);
        let light = arena.create_light().unwrap();
        assert_eq!(arena.light(light).unwrap().light_type, 3);
        assert_eq!(arena.state(light).unwrap().update_count, 43);
        assert_eq!(arena.light_random_state().state(), 2745024);
        arena.graph.set_light_type(light, 2).unwrap();
        arena.graph.set_light_mode(light, 0x11).unwrap();
        arena.graph.set_light_diffuse_power(light, 2.).unwrap();
        arena.graph.set_light_range(light, 25., 60.).unwrap();
        arena.graph.set_light_cone(light, 0.52, 1.22).unwrap();
        arena
            .graph
            .set_light_diffuse(light, [0.75, 0.5, 0.25])
            .unwrap();
        arena.graph.assign_position(light, [10., 20., 30.]).unwrap();
        arena.graph.update(light).unwrap();
        let state = arena.light(light).unwrap();
        assert_eq!(state.scaled_color, [1.5, 1., 0.5]);
        assert_eq!(state.position, [10., 20., 30.]);
        assert_eq!(state.range_difference, 35.);
        assert!(state.cone_radius > 0. && state.cone_reciprocal > 0.);
        assert_ne!(state.sphere_center[0], state.sphere_center[1]);
        let count = arena.state(light).unwrap().update_count;
        arena.graph.update(light).unwrap();
        assert_eq!(arena.state(light).unwrap().update_count, count + 1);
        arena.retain(light).unwrap();
        arena.release(light).unwrap();
        assert_eq!(arena.frame(light).unwrap().references, 1);
        arena.release(light).unwrap();
        assert!(arena.light(light).is_err());
    }
    #[test]
    fn set_light_type_linked_fog_to_fog_stays_supported() {
        let mut arena = ModelArena::default();
        let light = arena.create_light().unwrap();
        let sector = arena.create_sector_frame().unwrap();
        // Fog before linking, then fog<->fog linked runs Delete/Add natively.
        arena.graph.set_light_type(light, 5).unwrap();
        arena.sector_add_light(sector, light).unwrap();
        arena.graph.set_light_type(light, 6).unwrap();
        assert_eq!(arena.graph.light_state(light).unwrap().light_type, 6);
        // Counts and selection are untouched by the graph-only type store
        // (the sector Delete/Add effect is the caller's); native would end
        // with the re-added light selected — with a single fog light the
        // kept selection coincides (multi-fog edge disclosed in-port).
        assert_eq!(arena.sector(sector).unwrap().num_fog_lights, 1);
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(light));
        // fog<->non-fog linked still requires sector-owner reorder.
        assert!(arena.graph.set_light_type(light, 3).is_err());
        assert_eq!(arena.graph.light_state(light).unwrap().light_type, 6);
        assert_eq!(arena.sector(sector).unwrap().num_fog_lights, 1);
        arena.release(light).unwrap();
    }
    #[test]
    fn diffuse_cache_trigger_order_scales_then_dirties() {
        let mut arena = ModelArena::default();
        let light = arena.create_light().unwrap();
        let sector = arena.create_sector_frame().unwrap();
        let c0 = arena.state(light).unwrap().update_count;
        arena
            .graph
            .set_light_diffuse(light, [0.5, 0.25, 1.0])
            .unwrap();
        arena.graph.set_light_diffuse_power(light, 2.0).unwrap();
        let state = arena.graph.light_state(light).unwrap();
        assert_eq!(state.scaled_color, [1.0, 0.5, 2.0]);
        assert_eq!(arena.state(light).unwrap().update_count, c0.wrapping_add(2));
        assert_eq!(arena.sector_lights_changed(sector).unwrap_or(0), 0);
        arena.release(light).unwrap();
    }
    #[test]
    fn update_v_spot_caches_match_cone_and_range_math() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let light = arena.create_light().unwrap();
            arena.graph.set_light_type(light, 2).unwrap();
            arena.graph.set_light_range(light, 10.0, 50.0).unwrap();
            arena
                .graph
                .set_light_cone(light, 0.349066, 0.698132)
                .unwrap();
            arena.graph.assign_position(light, [4., 0., 0.]).unwrap();
            arena.graph.update(light).unwrap();
            let state = arena.light(light).unwrap();
            assert_eq!(state.range_difference, 40.0);
            assert_eq!(state.far_squared, 2500.0);
            assert!(state.outer_cos < state.inner_cos && state.cone_reciprocal > 0.0);
            assert!(state.cone_radius > 0.0 && state.sphere_radius > 0.0);
            arena.release(light).unwrap();
        });
    }
    #[test]
    fn duplicate_light_copies_fields_and_rebuilds_caches() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let src = arena.create_light().unwrap();
            arena.graph.set_light_type(src, 2).unwrap();
            arena
                .graph
                .set_light_diffuse(src, [0.75, 0.5, 0.25])
                .unwrap();
            arena.graph.set_light_diffuse_power(src, 2.0).unwrap();
            arena.graph.set_light_range(src, 5.0, 60.0).unwrap();
            arena.graph.set_light_cone(src, 0.2, 0.8).unwrap();
            arena.graph.update(src).unwrap();
            let dst = arena.create_light().unwrap();
            let fresh_radius = arena.graph.light_state(dst).unwrap().cone_radius;
            let fresh_sphere = arena.graph.light_state(dst).unwrap().sphere_radius;
            arena.duplicate_light(dst, src).unwrap();
            let (s, d) = (
                arena.graph.light_state(src).unwrap().clone(),
                arena.graph.light_state(dst).unwrap().clone(),
            );
            assert_eq!(d.color, s.color);
            assert_eq!(d.light_type, s.light_type);
            assert_eq!(d.intensity, s.intensity);
            assert_eq!(d.range, s.range);
            assert_eq!(d.cone, s.cone);
            // UpdateV tail rebuilds cosines and scaled ranges...
            assert_eq!(d.outer_cos, s.outer_cos);
            assert_eq!(d.far_squared, s.far_squared);
            assert_eq!(d.range_difference, s.range_difference);
            // ...but cone/sphere radii stay dst-construction values: no
            // UpdateV arm recomputes cone_radius (native-stale, disclosed).
            assert_eq!(d.cone_radius, fresh_radius);
            assert_eq!(d.sphere_radius, fresh_sphere);
            // ...and scaled_color stays constructor-default: no UpdateV arm
            // recomputes diffuse*power (native-stale, disclosed).
            assert_eq!(d.scaled_color, [1., 1., 1.]);
            assert!(d.sectors.is_empty()); // sectors never copied
            arena.release(src).unwrap();
            arena.release(dst).unwrap();
        });
    }
    #[test]
    fn load_image_failure_carries_light_prefix_and_no_cache_pollution() {
        let mut arena = ModelArena::default();
        let key = arena.create_light().unwrap();
        let before = arena.light(key).unwrap().scaled_color;
        let error = arena
            .graph
            .apply_light_load_image_checked(
                key,
                &lhr_assets::LightRecord {
                    mode: 0,
                    light_type: 3,
                    power: 1.0,
                    diffuse: [1., 1., 1.],
                    range: [1., 100.],
                    cone: [0.2, 0.8],
                },
                Err(FrameError::NumericDomain("Frame intake")),
            )
            .unwrap_err();
        assert!(format!("{error:?}").contains("Frame intake"));
        assert!(arena.graph.light_state(key).unwrap().fault.is_some());
        assert_eq!(arena.graph.light_state(key).unwrap().scaled_color, before);
        arena.release(key).unwrap();
    }
    #[test]
    fn set_light_on_is_idempotent_and_fog_reselects() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let fog = arena.create_light().unwrap();
        arena.graph.set_light_type(fog, 5).unwrap();
        arena.sector_add_light(sector, fog).unwrap();
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(fog));
        let count = arena.state(fog).unwrap().update_count;
        arena.set_light_on(fog, false).unwrap();
        assert_eq!(
            arena.state(fog).unwrap().update_count,
            count.wrapping_add(1)
        );
        arena.set_light_on(fog, false).unwrap(); // idempotent: no bump
        assert_eq!(
            arena.state(fog).unwrap().update_count,
            count.wrapping_add(1)
        );
        assert_ne!(arena.sector(sector).unwrap().fog_light, Some(fog));
        arena.set_light_on(fog, true).unwrap();
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(fog));
        arena.release(fog).unwrap();
    }
    #[test]
    fn fog_off_reselects_first_on_fog_in_sector_order() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let first = arena.create_light().unwrap();
        let second = arena.create_light().unwrap();
        arena.graph.set_light_type(first, 5).unwrap();
        arena.graph.set_light_type(second, 6).unwrap();
        arena.sector_add_light(sector, first).unwrap();
        arena.sector_add_light(sector, second).unwrap();
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(second));
        // Non-fog kind guard: no-op, selection untouched.
        let plain = arena.create_light().unwrap();
        arena.light_update_fog_selection(plain, true).unwrap();
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(second));
        // Turning off the selected fog reselects first ON fog in order.
        arena.set_light_on(second, false).unwrap();
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(first));
        arena.set_light_on(second, true).unwrap();
        assert_eq!(arena.sector(sector).unwrap().fog_light, Some(second));
        arena.release(first).unwrap();
        arena.release(second).unwrap();
        arena.release(plain).unwrap();
    }
}
