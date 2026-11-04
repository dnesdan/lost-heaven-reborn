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
    /// Original SetSpace flag machine, PC RVA 0x378b0 (contract: no Ghidra
    /// body; entry/mid probes + vtable xrefs; mafia-re I3d_object.cpp:453-507).
    /// The vertex bake (per-LOD LockVertices(0), matrix multiply,
    /// UpdateVertices/UpdateBBox/UpdateBoundVolume) and DetachMesh are caller
    /// backend contracts; this records the evidenced flag words and arm.
    /// World arm clears 0x40 first, then checks mesh (binary order);
    /// null-mesh keeps inv_matrix_valid clear. The local arm past WORLD_SPACE
    /// is unprobed (reference clears wbound validity there — disclosed).
    pub fn set_object_space(
        &mut self,
        object: ObjectResourceKey,
        world: bool,
    ) -> Result<ObjectSpaceTransition, ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let mesh = record.mesh;
        let slot = self.index(object.arena, object.slot)?;
        let target = self.objects[slot]
            .as_mut()
            .ok_or(ObjectError::UnknownObject(object))?;
        if target.world_space {
            if world {
                return Ok(ObjectSpaceTransition::AlreadyWorld);
            }
            target.world_space = false;
            if mesh.is_some() {
                target.inv_matrix_valid = true;
            }
            return Ok(ObjectSpaceTransition::BakeToLocal);
        }
        if !world {
            return Ok(ObjectSpaceTransition::AlreadyLocal);
        }
        target.world_space = true;
        Ok(ObjectSpaceTransition::BakeToWorld)
    }
    /// Original SetLightness, PC RVA 0x37d50 (contract: no Ghidra body;
    /// entry/mid/RET probes + epsilon word 0x322bcc77; mafia-re
    /// I3d_object.cpp:509-524). The epsilon is the evidenced bit word
    /// compared signed (SETL): differences at or below it skip the store —
    /// including signed-zero; canonical NaN bits are positive-huge, so NaN
    /// STORES (corrected from the memo's sign analysis — disclosed). Host f32
    /// subtraction is single-rounded (x87 double-rounding can differ one ulp
    /// in halfway cases, absorbed by the epsilon). No clamp: any float
    /// stores on the update path. Effect order per slot: lights_changed
    /// (+4) then light_key (+0xC), slots descending.
    pub fn set_object_lightness(
        &mut self,
        object: ObjectResourceKey,
        lightness: f32,
    ) -> Result<(), ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let Some(mesh) = record.mesh else {
            return Ok(());
        };
        if ((lightness - record.lightness).abs().to_bits() as i32) < 0x322b_cc77 {
            return Ok(());
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
        let lods = record.lods;
        let slot = self.index(object.arena, object.slot)?;
        self.objects[slot]
            .as_mut()
            .ok_or(ObjectError::UnknownObject(object))?
            .lightness = lightness;
        for key in lods[..count].iter().rev().flatten().copied() {
            let slot = self.index(key.arena, key.slot)?;
            let lod = self.lods[slot]
                .as_mut()
                .ok_or(ObjectError::UnknownLod(key))?;
            lod.lights_changed = u32::MAX;
            lod.light_key = u32::MAX;
        }
        Ok(())
    }
    /// Original SetTransparency, PC RVA 0x37dd0 (differential: Ghidra body
    /// 138 B, 9 BB; mafia-re I3d_object.cpp:526-541). x87 FISTP with default
    /// control word: nearest-even, 0x80000000 outside i32 range (incl. NaN);
    /// host single-rounding can differ one ulp in halfway cases (disclosed).
    /// Stores the new float even when the packed byte is equal; only LOD
    /// alpha_dirty flips across byte changes. No domain gate, no clamp.
    pub fn set_object_transparency(
        &mut self,
        object: ObjectResourceKey,
        transparency: f32,
    ) -> Result<(), ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let Some(mesh) = record.mesh else {
            return Ok(());
        };
        let count = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(|e| ObjectError::Managed(e.into()))?
            .num_lods as usize;
        if count > record.lods.len() {
            return Err(ObjectLodError::InvalidLevels);
        }
        let changed = packed_alpha_byte(record.transparency) != packed_alpha_byte(transparency);
        let lods = record.lods;
        let slot = self.index(object.arena, object.slot)?;
        self.objects[slot]
            .as_mut()
            .ok_or(ObjectError::UnknownObject(object))?
            .transparency = transparency;
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
    /// Original ResetLightInfo, PC RVA 0x37e60 (contract: no Ghidra body;
    /// entry/mid/RET probes + double null-guard order; mafia-re
    /// I3d_object.cpp:543-555). Marks every in-range LOD unconditionally: no
    /// float compare, no early-out. Word order per slot is lights_changed
    /// (+4) then light_key (+0xC); slots iterate descending.
    pub fn reset_object_light_info(
        &mut self,
        object: ObjectResourceKey,
    ) -> Result<(), ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let Some(mesh) = record.mesh else {
            return Ok(());
        };
        let count = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(|e| ObjectError::Managed(e.into()))?
            .num_lods as usize;
        if count > record.lods.len() {
            return Err(ObjectLodError::InvalidLevels);
        }
        let lods = record.lods;
        for key in lods[..count].iter().rev().flatten().copied() {
            let slot = self.index(key.arena, key.slot)?;
            let lod = self.lods[slot]
                .as_mut()
                .ok_or(ObjectError::UnknownLod(key))?;
            lod.lights_changed = u32::MAX;
            lod.light_key = u32::MAX;
        }
        Ok(())
    }
    /// Original LockVertices CPU operands, PC RVA 0x37f30 (contract: no
    /// Ghidra body; entry + RET-0xC guard + CALL-veneer probes; mafia-re
    /// I3d_object.cpp:574-582). Resolves the evidenced guard order; the
    /// device Lock itself is the existing lock_vertex_buffer backend
    /// contract. None is the NULL word (empty slot or no buffer). The binary
    /// takes a signed int with no bounds check (negative is UB there); the
    /// host u32 API rejects by construction, and >= 10 mirrors the LOD
    /// index bound used across the owner (InvalidIndex).
    pub fn resolve_object_vertex_lock(
        &self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<Option<ObjectVertexLockTarget>, ObjectError> {
        self.available()?;
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index));
        }
        let record = self.object(object)?;
        let Some(lod) = record.lods[index as usize] else {
            return Ok(None);
        };
        let lod = self.lod(lod)?;
        let buffer = lod.vertex_buffer;
        if buffer.buffer.is_none() {
            return Ok(None);
        }
        Ok(Some(ObjectVertexLockTarget {
            buffer,
            base: buffer.base,
        }))
    }
    /// Original m_RenderFGroup LOD select, PC RVA 0x39280 (contract: entry +
    /// TEST + JZ probes + LOD-select prologue; device calls stay backend
    /// contracts; mafia-re I3d_object.cpp:868-883). MATF_LOD0 (0x800,
    /// I3D.h:3803) forces slot 0, else the current LOD. Proxy resolution
    /// (m_UserData) is a caller backend contract; this operates on the
    /// resolved object. The count (cull sentinel, == num_lods) selects no
    /// level and draws nothing: it returns Ok(count) rather than an error,
    /// mirroring the get_object_lod sentinel convention (a strict `>=` bound
    /// would reject the sentinel the engine stores — disclosed choice).
    /// Callers must never use the result as a LOD index (count is one past
    /// the last slot); the render submitter draws nothing for it.
    pub fn select_render_lod_index(
        &mut self,
        object: ObjectResourceKey,
        use_lod0: bool,
    ) -> Result<u32, ObjectLodError> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectLodError::ObjectNotAlive);
        }
        let Some(mesh) = record.mesh else {
            return Err(ObjectLodError::InvalidLevels);
        };
        let count = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(|e| ObjectError::Managed(e.into()))?
            .num_lods as usize;
        if count == 0 || count > record.lods.len() {
            return Err(ObjectLodError::InvalidLevels);
        }
        if use_lod0 {
            return Ok(0);
        }
        let current = record.cur_lod;
        if current as usize > count {
            return Err(ObjectLodError::InvalidLevels);
        }
        Ok(current)
    }
    /// Original I3D_object::LoadImage order, PC RVA 0x3a400 (differential:
    /// Ghidra body 37 B, 3 BB; mafia-re I3d_object.cpp:1126-1135). The
    /// base-frame intake and mesh image arrive as caller backend contracts,
    /// then attach with preserve_lods=false, then the native Release(m)
    /// word. Host ownership mapping: SetMesh retains once and releases the
    /// previous mesh — the net effect of native AddRef-in-SetMesh plus the
    /// caller Release; with mesh=None no backend release call is made. An
    /// intake failure returns the reached error with the object untouched
    /// (objects carry no sticky fault slot).
    pub fn apply_object_load_image_checked<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        intake: Result<(), ObjectOperationError<B::Error>>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        intake?;
        self.set_object_mesh(object, mesh, false, backend)
    }
}
/// LS3DF 0x100378b0 flag-machine arms (contract).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSpaceTransition {
    AlreadyWorld,
    BakeToLocal,
    AlreadyLocal,
    BakeToWorld,
}
/// CPU operands for the LockVertices device call (PC RVA 0x37f30). The device
/// Lock itself is the existing lock_vertex_buffer backend contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectVertexLockTarget {
    pub buffer: crate::visual_resources::CompleteVertexBuffer,
    pub base: u32,
}
/// x87 FISTP with the default control word (PC RVA 0x37dd0): nearest-even,
/// 0x80000000 outside i32 range (including NaN). Host single-rounding can
/// differ one ulp from the 80-bit intermediate in halfway cases (disclosed).
/// Range-gate note: the 2_147_483_647./2_147_483_648. literals round to 2^31,
/// so host inputs mapping into (2147483520, 2147483648) classify against the
/// rounded bound where FISTP-indefinite starts exactly at 2^31 — negligible
/// in the alpha domain, disclosed.
pub fn packed_alpha_byte(value: f32) -> i32 {
    let rounded = (value * 255.).round_ties_even();
    if rounded.is_nan() || rounded > 2_147_483_647. || rounded < -2_147_483_648. {
        0x8000_0000u32 as i32
    } else {
        rounded as i32
    }
}
/// LS3DF 0x10039280 alpha product in reference order (I3d_object.cpp:958):
/// (fade * transparency) * mat_alpha, checked (no fuse, no clamping).
pub fn render_total_alpha(
    fade: f32,
    transparency: f32,
    material_alpha: f32,
) -> Result<f32, ObjectLodError> {
    let partial = numeric(mul(fade, transparency))?;
    numeric(mul(partial, material_alpha))
}
/// LS3DF 0x10039280 reset-bit protocol (I3d_object.cpp:885,941-954,1118-1123):
/// 1 TEXCOORDINDEX, 2 ADDRESSU, 4 ADDRESSV. Pure CPU word; the stage-state
/// writes are device backend contracts.
pub fn render_stage_reset_flags(
    address_u_clamp: bool,
    address_v_clamp: bool,
    texcoord_index_override: bool,
) -> u32 {
    (u32::from(texcoord_index_override))
        | (u32::from(address_u_clamp) << 1)
        | (u32::from(address_v_clamp) << 2)
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
/// I3D_singlemesh::m_AddFGroups (also inherited by Singlemorph) and
/// I3D_billboard::m_AddFGroups use the full previous LOD distance when
/// starting the last-level fade. Its current-last half distance is unchanged.
pub(crate) fn render_derived_visual_fade(
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
