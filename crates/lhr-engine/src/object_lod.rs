//! Ordinary source GetObjectLOD and SetFade state on the existing object owner.
//! mafia-re 29b65b0, render/render.cpp:117-183, I3d_object.cpp:559-573.
//! Callback/texture-driven/proxy/world-space domains are admitted by the caller
//! separately. No native parity credit is added by this source implementation.
use super::*;
use crate::native_matrix::{
    checked_f32_add as add, checked_f32_div as div, checked_f32_mul as mul, checked_f32_sub as sub,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectLodError {
    Object(ObjectError),
    NumericDomain,
    InvalidLevels,
    ObjectNotAlive,
}
impl From<ObjectError> for ObjectLodError {
    fn from(e: ObjectError) -> Self {
        Self::Object(e)
    }
}
fn numeric<T>(
    value: Result<T, crate::native_matrix::MatrixArithmeticUnavailable>,
) -> Result<T, ObjectLodError> {
    value.map_err(|_| ObjectLodError::NumericDomain)
}
fn scaled_distance(
    distance_squared: f32,
    quality: f32,
    world_scale: f32,
) -> Result<f32, ObjectLodError> {
    let scale = numeric(mul(quality, world_scale))?;
    numeric(div(distance_squared, numeric(mul(scale, scale))?))
}
impl ObjectResources {
    /// Source GetObjectLOD ordinary branch. Returns num_lods for the explicit
    /// render-cull sentinel. The single missing/nonpositive level early return
    /// does not read distance/scale/quality or write cur_lod.
    pub fn get_object_lod(
        &mut self,
        object: ObjectResourceKey,
        distance_squared: f32,
        quality: f32,
        world_scale: f32,
    ) -> Result<u32, ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let Some(mesh) = record.mesh else {
            return Err(ObjectLodError::InvalidLevels);
        };
        let mesh = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(|e| ObjectError::Managed(e.into()))?;
        let count = mesh.num_lods as usize;
        if count == 0 || count > mesh.levels.len() {
            return Err(ObjectLodError::InvalidLevels);
        }
        let distance = |index: usize| -> Result<Option<f32>, ObjectLodError> {
            mesh.levels[index]
                .map(|key| {
                    self.managed
                        .resources()
                        .mesh_level(key)
                        .map(|level| level.lod_dist)
                        .map_err(|e| ObjectError::Managed(e.into()).into())
                })
                .transpose()
        };
        let index = if count == 1 {
            let Some(limit) = distance(0)? else {
                return Ok(0);
            };
            if limit <= 0. {
                return Ok(0);
            }
            if scaled_distance(distance_squared, quality, world_scale)? > limit {
                1
            } else {
                0
            }
        } else {
            let value = scaled_distance(distance_squared, quality, world_scale)?;
            let mut selected = count - 1;
            for index in 0..count - 1 {
                if distance(index)?.is_some_and(|limit| value <= limit) {
                    selected = index;
                    break;
                }
            }
            if selected == count - 1
                && distance(selected)?.is_some_and(|limit| limit > 0. && value > limit)
            {
                count
            } else {
                selected
            }
        };
        let index = index as u32;
        let slot = self.index(object.arena, object.slot)?;
        self.objects[slot]
            .as_mut()
            .ok_or(ObjectError::UnknownObject(object))?
            .cur_lod = index;
        Ok(index)
    }
    /// Source SetFade stores the new float even when packed alpha is equal.
    /// Existing private LOD slots become alpha-dirty only across byte changes.
    pub(crate) fn set_render_fade(
        &mut self,
        object: ObjectResourceKey,
        fade: f32,
    ) -> Result<(), ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let Some(mesh) = record.mesh else {
            return Ok(());
        };
        if !fade.is_finite() || !(0. ..=1.).contains(&fade) || (fade != 0. && !fade.is_normal()) {
            return Err(ObjectLodError::NumericDomain);
        }
        let count = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(|e| ObjectError::Managed(e.into()))?
            .num_lods as usize;
        if count > record.lods.len() {
            return Err(ObjectLodError::InvalidLevels);
        }
        let changed = (record.fade * 255.).round_ties_even() != (fade * 255.).round_ties_even();
        let lods = record.lods;
        let slot = self.index(object.arena, object.slot)?;
        self.objects[slot]
            .as_mut()
            .ok_or(ObjectError::UnknownObject(object))?
            .fade = fade;
        if changed {
            for key in lods[..count].iter().rev().flatten().copied() {
                let slot = self.index(key.arena, key.slot)?;
                self.lods[slot]
                    .as_mut()
                    .ok_or(ObjectError::UnknownLod(key))?
                    .alpha_dirty = true;
            }
        }
        Ok(())
    }
}
/// Source last-LOD fade interval. Only call after a renderable level is selected.
pub(crate) fn render_fade(
    distance_squared: f32,
    quality: f32,
    world_scale: f32,
    index: usize,
    distances: &[Option<f32>],
) -> Result<f32, ObjectLodError> {
    let last = distances
        .len()
        .checked_sub(1)
        .ok_or(ObjectLodError::InvalidLevels)?;
    let limit = distances[index].ok_or(ObjectLodError::InvalidLevels)?;
    if index != last || limit <= 0. {
        return Ok(1.);
    }
    let mut start = numeric(mul(limit, 0.5))?;
    if last != 0 {
        start = numeric(add(
            start,
            numeric(mul(
                distances[last - 1].ok_or(ObjectLodError::InvalidLevels)?,
                0.5,
            ))?,
        ))?;
    }
    let distance = scaled_distance(distance_squared, quality, world_scale)?;
    let fade = numeric(sub(
        1.,
        numeric(div(
            numeric(sub(distance, start))?,
            numeric(sub(limit, start))?,
        ))?,
    ))?;
    Ok(if fade > 1. { 1. } else { fade })
}
/// I3D_billboard::m_AddFGroups uses the full previous LOD distance when
/// starting the last-level fade. Its current-last half distance is unchanged.
pub(crate) fn render_billboard_fade(
    distance_squared: f32,
    quality: f32,
    world_scale: f32,
    index: usize,
    distances: &[Option<f32>],
) -> Result<f32, ObjectLodError> {
    let last = distances
        .len()
        .checked_sub(1)
        .ok_or(ObjectLodError::InvalidLevels)?;
    let limit = distances[index].ok_or(ObjectLodError::InvalidLevels)?;
    if index != last || limit <= 0. {
        return Ok(1.);
    }
    let mut start = numeric(mul(limit, 0.5))?;
    if last != 0 {
        start = numeric(add(
            start,
            distances[last - 1].ok_or(ObjectLodError::InvalidLevels)?,
        ))?;
    }
    let distance = scaled_distance(distance_squared, quality, world_scale)?;
    let fade = numeric(sub(
        1.,
        numeric(div(
            numeric(sub(distance, start))?,
            numeric(sub(limit, start))?,
        ))?,
    ))?;
    Ok(if fade > 1. { 1. } else { fade })
}
