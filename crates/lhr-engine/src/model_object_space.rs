//! `I3D_object::SetSpace(true)` and the mesh LOD0 vertex lock over the
//! arena's live visual owners, as `g_collision::LoadCollision` (GAME
//! 0x5c2b70) reaches them through a link's visual frame.
//!
//! LS3DF.dll SHA-256 e211c850…f178a6. `SetSpace` is visual vtable +0x50; the
//! plain object's (0x1009c348) and the lit object's (0x1009bd70) vtables both
//! hold `I3D_object::SetSpace` at RVA 0x378b0 (mafia-re fe7e746
//! ls3df/I3D/visuals/I3d_object.cpp:455-506). Ghidra forms no procedure
//! there; the evidence is the linear instruction walk 0x378b0..0x37d3f.
//! World arm (0x37b09..0x37d3f):
//! - `world` false → return; `vis_flags & 0x40` (already world) returned at
//!   0x378c7/0x378cf before anything else.
//! - `!(vis_flags & 0x40000)` → `UpdateWBoundProc` (0x5a510, call 0x37b19).
//! - mesh (+0x1e0) null → `vis_flags = (vis_flags & ~0x40000) | 0x40`.
//! - `DetachMesh` (vtable +0x74 = 0x36a70, 0x37b2f), then the mesh is reread.
//! - The render matrix: identity when 0x40, else the frame matrix after the
//!   `+0xac & 0x20`-gated `UpdateWMatrixProc` (0x1c7d0), copied once.
//! - For `i` below the mesh's `num_lods` (reread every iteration): `GetLOD(i)`
//!   (mesh vtable +4 = 0x313f0); a null level is skipped; `LockVertices(0)`
//!   (level vtable +4 = 0x30e80); `num_vertices` (+8) read after the lock;
//!   per vertex `pos *= m` (out-of-line 0x2e920), `normal =
//!   normal.RotateByMatrix(m)` (0x2ea10, through a stack temporary) and the
//!   inlined `S_vector::Normalize`; then `UpdateVertices(i)` (object vtable
//!   +0x78: 0x36a50 plain, 0x21790 lit) and `UpdateBBox` (level vtable +0x28
//!   = 0x30a50). The lock is never released here.
//! - Mesh `UpdateBoundVolume` (vtable +0x14 = 0x31400), then
//!   `vis_flags = (vis_flags & ~0x40000) | 0x40`, reread from memory.
//!
//! A second `SetSpace(true)` is a no-op: the first test returns.
//!
//! Inlined `Normalize` (0x37be0..0x37cd5): `len2 = (x*x + y*y) + z*z` stored
//! to f32; `|len2 - 1| < 1e-8` (f64 0x1009b4c8, the f32 word 0x322bcc77
//! widened) keeps the normal; `len2 < 1e-8` (f32 0x1009b308) replaces the
//! first non-zero of x, z, y by its sign (±1, 0x1009b304/0x1009b314; a zero
//! y becomes +1) and leaves the other two; otherwise every axis is multiplied
//! by `1 / sqrt(len2)` in x, y, z order.
//!
//! Numeric policy: the game runs these x87 operations under PC24/RN. Each
//! operation rounds to a 24-bit significand with the register's extended
//! exponent, and binary32 rounding happens only at the `fstp dword` stores,
//! as the instructions do; a tiny product such as a 1e-20 normal component
//! squared therefore stays normal in the register and vanishes in the sum,
//! as on the original. A non-finite load, result or store stops the bake
//! with [`ModelError::SetSpaceNumeric`], keeping the vertices already
//! written.
//!
//! The arena keeps no `vis_bound_w` cache: `UpdateWBoundProc`'s computation
//! runs (and can stop the bake) but its result is not stored, and the final
//! flag store leaves the native cache invalid in any case. Visuals other than
//! the plain and lit objects stop with [`ModelError::SetSpaceUnsupportedVisual`].
use super::*;
use crate::game_collision::LodVertexBuffer;
use crate::i3d_object::ObjectVisualType;

/// `VISF_WORLD_SPACE` (I3D.h:2706).
const VISF_WORLD_SPACE: u32 = 0x40;
/// `VISF_WBOUND_VALID` (I3D.h:2719).
const VISF_WBOUND_VALID: u32 = 0x40000;
/// 1e-8 as the f32 word at 0x1009b308 (the f64 at 0x1009b4c8 is its widening).
const NORMALIZE_EPSILON: f32 = f32::from_bits(0x322b_cc77);

/// What `I3D_object::SetSpace(true)` did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectSpaceWorld {
    /// The visual was already in world space: no effect.
    AlreadyWorld,
    /// No mesh: only the flags were set.
    Meshless,
    /// The mesh's levels were transformed into world space.
    Baked {
        /// `DetachMesh` replaced a shared mesh with a private copy.
        detached: bool,
        levels: u32,
        vertices: u32,
    },
}

/// One x87 operation result under PC24/RN: the exact result rounded to a
/// 24-bit significand, keeping the register's extended exponent range. The
/// binary64 carrier rounds first to 53 bits; that double rounding is
/// innocuous for +, -, *, / and sqrt because 53 >= 2*24 + 2.
fn pc24(value: f64) -> Result<f64, ()> {
    if !value.is_finite() {
        return Err(());
    }
    if value == 0.0 {
        return Ok(value);
    }
    let bits = value.to_bits();
    let discarded = bits & ((1u64 << 29) - 1);
    let mut retained = bits >> 29;
    if discarded > 1 << 28 || discarded == 1 << 28 && retained & 1 != 0 {
        retained += 1;
    }
    Ok(f64::from_bits(retained << 29))
}
/// `fld dword`: exact, a finite f32 only (the bake has no evidence for
/// infinities or NaNs reaching it).
fn load(value: f32) -> Result<f64, ()> {
    value.is_finite().then_some(f64::from(value)).ok_or(())
}
/// `fstp dword`: the register value rounded once to binary32 (gradual
/// underflow included); an overflow stops.
fn store(value: f64) -> Result<f32, ()> {
    let stored = value as f32;
    stored.is_finite().then_some(stored).ok_or(())
}
fn fmul(a: f64, b: f64) -> Result<f64, ()> {
    pc24(a * b)
}
fn fadd(a: f64, b: f64) -> Result<f64, ()> {
    pc24(a + b)
}

/// `S_vector::operator*=(const S_matrix &)`, LS3DF 0x2e920: in registers,
/// x' = ((m[2][0]*z + m[1][0]*y) + m[0][0]*x) + m[3][0] and, for y and z,
/// ((m[2][a]*z + m[0][a]*x) + m[1][a]*y) + m[3][a]; every input is loaded
/// before the three stores.
fn transform_position(p: [f32; 3], m: &[f32; 16]) -> Result<[f32; 3], ()> {
    let [x, y, z] = [load(p[0])?, load(p[1])?, load(p[2])?];
    let e = |i: usize| load(m[i]);
    let x_axis = fadd(
        fadd(fadd(fmul(e(8)?, z)?, fmul(e(4)?, y)?)?, fmul(e(0)?, x)?)?,
        e(12)?,
    )?;
    let axis = |a: usize| -> Result<f64, ()> {
        fadd(
            fadd(
                fadd(fmul(e(8 + a)?, z)?, fmul(e(a)?, x)?)?,
                fmul(e(4 + a)?, y)?,
            )?,
            e(12 + a)?,
        )
    };
    Ok([store(x_axis)?, store(axis(1)?)?, store(axis(2)?)?])
}

/// `S_vector::RotateByMatrix(const S_matrix &)`, LS3DF 0x2ea10: each axis
/// `(x*m[0][a] + y*m[1][a]) + z*m[2][a]` in registers, stored to a stack
/// temporary; no translation.
fn rotate_normal(n: [f32; 3], m: &[f32; 16]) -> Result<[f32; 3], ()> {
    let [x, y, z] = [load(n[0])?, load(n[1])?, load(n[2])?];
    let mut out = [0.0; 3];
    for (axis, value) in out.iter_mut().enumerate() {
        *value = store(fadd(
            fadd(fmul(x, load(m[axis])?)?, fmul(y, load(m[4 + axis])?)?)?,
            fmul(z, load(m[8 + axis])?)?,
        )?)?;
    }
    Ok(out)
}

/// The `S_vector::Normalize` inlined in `SetSpace` (see the module notes).
fn normalize_normal(n: [f32; 3]) -> Result<[f32; 3], ()> {
    let [x, y, z] = [load(n[0])?, load(n[1])?, load(n[2])?];
    let len2 = store(fadd(fadd(fmul(x, x)?, fmul(y, y)?)?, fmul(z, z)?)?)?;
    let len2_wide = f64::from(len2);
    if fadd(len2_wide, -1.0)?.abs() < f64::from(NORMALIZE_EPSILON) {
        return Ok(n);
    }
    let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
    if len2 < NORMALIZE_EPSILON {
        let mut out = n;
        if n[0] != 0.0 {
            out[0] = sign(n[0]);
        } else if n[2] != 0.0 {
            out[2] = sign(n[2]);
        } else {
            out[1] = sign(n[1]);
        }
        return Ok(out);
    }
    let reciprocal = pc24(1.0 / pc24(len2_wide.sqrt())?)?;
    Ok([
        store(fmul(reciprocal, x)?)?,
        store(fmul(reciprocal, y)?)?,
        store(fmul(reciprocal, z)?)?,
    ])
}

/// One vertex of the bake loop: position, then normal.
pub(crate) fn bake_world_vertex(
    position: [f32; 3],
    normal: [f32; 3],
    matrix: &SMatrix,
) -> Result<[[f32; 3]; 2], ()> {
    let position = transform_position(position, &matrix.elements)?;
    let normal = normalize_normal(rotate_normal(normal, &matrix.elements)?)?;
    Ok([position, normal])
}

impl ModelArena {
    fn space_visual_object(
        &self,
        key: FrameKey,
    ) -> Result<(crate::i3d_object::ObjectResourceKey, ObjectVisualType), ModelError> {
        let unsupported = ModelError::SetSpaceUnsupportedVisual(key);
        if self.state(key)?.kind != FrameKind::Mesh {
            return Err(unsupported);
        }
        let object = self.frame(key)?.object_resources.ok_or(unsupported)?;
        let visual = self
            .loaded_visual_resources()
            .ok_or(ModelError::SetSpaceUnsupportedVisual(key))?
            .objects()
            .object(object)
            .map_err(|e| ModelError::LoadedVisual(e.into()))?
            .visual_type();
        Ok((object, visual))
    }

    /// `I3D_object::SetSpace(true)`, LS3DF RVA 0x378b0, world arm, over the
    /// frame's live object, mesh and vertex storage (module notes). The
    /// frame's `vis_flags` word carries `VISF_WORLD_SPACE`, which
    /// `GetRenderMatrix` (0x2ad00) and `UpdateV` read; the object record's
    /// world-space word is set with it. A host stop keeps every effect
    /// before it, as native has no failure path.
    pub fn set_object_space_world(
        &mut self,
        key: FrameKey,
    ) -> Result<ObjectSpaceWorld, ModelError> {
        let (object, visual) = self.space_visual_object(key)?;
        if !matches!(
            visual,
            ObjectVisualType::Plain | ObjectVisualType::LitUnconstructed
        ) {
            return Err(ModelError::SetSpaceUnsupportedVisual(key));
        }
        let flags = self.state(key)?.visual_flags;
        if flags & VISF_WORLD_SPACE != 0 {
            return Ok(ObjectSpaceWorld::AlreadyWorld);
        }
        if flags & VISF_WBOUND_VALID == 0 {
            self.visual_world_bounds(key)?;
        }
        let loaded = ModelError::LoadedVisual;
        fn resources(
            arena: &mut ModelArena,
            key: FrameKey,
        ) -> Result<&mut crate::loaded_visual_resources::LoadedVisualResources, ModelError>
        {
            arena
                .loaded_visual_resources_mut()
                .ok_or(ModelError::SetSpaceUnsupportedVisual(key))
        }
        let mesh = resources(self, key)?
            .objects()
            .object(object)
            .map_err(|e| loaded(e.into()))?
            .mesh;
        let outcome = if mesh.is_none() {
            ObjectSpaceWorld::Meshless
        } else {
            let detached = resources(self, key)?
                .game_detach_object_mesh(object)
                .map_err(loaded)?;
            let matrix = self.graph.visual_render_matrix(key)?;
            let owner = resources(self, key)?;
            let mesh = owner
                .objects()
                .object(object)
                .map_err(|e| loaded(e.into()))?
                .mesh
                .ok_or(ModelError::SetSpaceUnsupportedVisual(key))?;
            let (mut levels, mut vertices) = (0u32, 0u32);
            let mut index = 0u32;
            while index < owner.game_mesh_word(mesh, 8).map_err(loaded)? {
                let level = owner
                    .objects()
                    .get_lod(mesh, index)
                    .map_err(|e| loaded(e.into()))?;
                if let Some(level) = level {
                    let locked = owner.game_lock_level_vertices(level, 0).map_err(loaded)?;
                    let count = owner.game_level_word(level, 8).map_err(loaded)?;
                    if count != 0 {
                        if !locked {
                            return Err(ModelError::VertexLockUnavailable { frame: key, index });
                        }
                        let window = owner.game_level_vertex_window(level).map_err(loaded)?;
                        for vertex in 0..count as usize {
                            let [position, normal] = owner
                                .game_vertex_position_normal(window, vertex)
                                .map_err(loaded)?;
                            let baked = bake_world_vertex(position, normal, &matrix)
                                .map_err(|()| ModelError::SetSpaceNumeric(key))?;
                            owner
                                .game_write_vertex_position_normal(window, vertex, baked)
                                .map_err(loaded)?;
                            vertices += 1;
                        }
                    }
                    owner
                        .game_update_object_lod_vertices(object, index)
                        .map_err(loaded)?;
                    owner.game_update_level_bbox(level).map_err(loaded)?;
                    levels += 1;
                }
                index += 1;
            }
            owner.game_update_mesh_bounds(mesh).map_err(loaded)?;
            ObjectSpaceWorld::Baked {
                detached,
                levels,
                vertices,
            }
        };
        self.graph
            .set_visual_flag_words(key, VISF_WBOUND_VALID, VISF_WORLD_SPACE)?;
        resources(self, key)?
            .game_mark_object_world_space(object)
            .map_err(loaded)?;
        Ok(outcome)
    }

    /// LoadCollision's LOD0 lock for a type-1 frame (GAME 0x5c2f05..0x5c2f4e):
    /// `None` for a mirror (`'MIRR'` at +0x1d4) or a visual without mesh
    /// (+0x1e0), else `GetMesh()->GetLOD(0)->LockVertices(1)` (mesh vtable
    /// +4 = LS3DF 0x313f0, level vtable +4 = 0x30e80), never released. The
    /// returned view shares the level's live vertex storage. A null lock
    /// result is the native null pointer (`None`). A null LOD0, which native
    /// would call through, and a visual without an object owner stop.
    pub fn lock_visual_lod0_vertices(
        &mut self,
        key: FrameKey,
    ) -> Result<Option<LodVertexBuffer>, ModelError> {
        if self.frame(key)?.mirror.is_some() {
            return Ok(None);
        }
        let (object, _) = self.space_visual_object(key)?;
        let owner = self
            .loaded_visual_resources_mut()
            .ok_or(ModelError::SetSpaceUnsupportedVisual(key))?;
        let loaded = ModelError::LoadedVisual;
        let Some(mesh) = owner
            .objects()
            .object(object)
            .map_err(|e| loaded(e.into()))?
            .mesh
        else {
            return Ok(None);
        };
        let level = owner
            .objects()
            .get_lod(mesh, 0)
            .map_err(|e| loaded(e.into()))?
            .ok_or(ModelError::MeshLodUnavailable {
                frame: key,
                index: 0,
            })?;
        if !owner.game_lock_level_vertices(level, 1).map_err(loaded)? {
            return Ok(None);
        }
        owner
            .game_level_vertex_view(level)
            .map(Some)
            .map_err(loaded)
    }
}

#[cfg(test)]
#[path = "model_object_space_tests.rs"]
mod tests;
