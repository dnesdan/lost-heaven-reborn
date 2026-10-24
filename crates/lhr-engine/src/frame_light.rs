//! CPU Light constructor, local setters and unregistered light UpdateV.
//! Actual PS2 I3D_light symbols corroborate SetLightType, SetDiffuse,
//! SetDiffusePower, SetRange, SetCone, UpdateV and SetMode; PC layouts govern.
//! Original PC entries:201f0,20760,20800,20930,20a90,20b20,20c30,20d10,21440.
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
            _ => return Err(FrameError::UnsupportedFrameUpdate(key)),
        }
        Ok(())
    }
    fn set_light_type_inner(&mut self, key: FrameKey, kind: u32) -> Result<(), FrameError> {
        let old = self.light_state(key)?.light_type;
        if old == kind {
            return Ok(());
        }
        if !(1..=4).contains(&kind) {
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
        self.light_scaled_range(key)?;
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
    pub fn light_random_state(&mut self) -> &mut LightCrtRandom {
        &mut self.light_random
    }
    pub fn create_light(&mut self) -> Result<FrameKey, ModelError> {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Light;
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
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
}
