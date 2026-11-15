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
//! `UpdateWBoundProc` is the frame's real operation
//! ([`ModelArena::update_visual_wbound_proc`]): it sets 0x40000, refills the
//! `vis_bound_w` cache and, through `UpdateBoundProc` 0x5a4f0, sets
//! `VISF_BOUND_VALID` 0x20000, which the final store keeps; a baked visual
//! leaves `0x20040` set. Visuals other than the plain and lit objects stop
//! with [`ModelError::SetSpaceUnsupportedVisual`].
//!
//! Store order (linear walk): `pos *= m` stores z, x, y (0x2e96f..0x2e976);
//! the rotated normal is copied word by word from the stack temporary
//! (0x37bce..0x37bdd); Normalize reloads it and stores x, y, z
//! (0x37cc9..0x37cd5). The bake writes each word through the vertex owner in
//! that order, so a host stop keeps exactly the stores before it. A visual
//! whose bake stopped is marked and every later `SetSpace(true)` on it is
//! refused ([`ModelError::SetSpaceInterrupted`]) before any effect, since the
//! stored vertices are partly world space already.
//!
//! Every x87 operation and `fstp dword` store records the sticky precision
//! exception in the active [`crate::native_precision::Pc24PrecisionState`]
//! scope (the mission session's, across LoadCollision).
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
/// infinities or NaNs reaching it). Loading never raises precision.
fn load(value: f32) -> Result<f64, ()> {
    value.is_finite().then_some(f64::from(value)).ok_or(())
}
/// `fstp dword`: the register value rounded once to binary32 (gradual
/// underflow included), raising precision when that rounds; an overflow
/// stops before the store.
fn store(value: f64) -> Result<f32, ()> {
    let stored = value as f32;
    if !stored.is_finite() {
        return Err(());
    }
    crate::native_precision::record_binary32_store(value, stored);
    Ok(stored)
}
fn fmul(a: f64, b: f64) -> Result<f64, ()> {
    let result = pc24(a * b)?;
    crate::native_precision::record_wide_pc24_mul(a, b, result);
    Ok(result)
}
fn fadd(a: f64, b: f64) -> Result<f64, ()> {
    let result = pc24(a + b)?;
    crate::native_precision::record_wide_pc24_add(a, b, result);
    Ok(result)
}
fn fsub(a: f64, b: f64) -> Result<f64, ()> {
    let result = pc24(a - b)?;
    crate::native_precision::record_wide_pc24_sub(a, b, result);
    Ok(result)
}
fn fsqrt(a: f64) -> Result<f64, ()> {
    let result = pc24(a.sqrt())?;
    crate::native_precision::record_wide_pc24_sqrt(a, result);
    Ok(result)
}
fn fdiv(a: f64, b: f64) -> Result<f64, ()> {
    let result = pc24(a / b)?;
    crate::native_precision::record_wide_pc24_div(a, b, result);
    Ok(result)
}

/// `S_vector::operator*=(const S_matrix &)`, LS3DF 0x2e920, register half:
/// x' = ((m[2][0]*z + m[1][0]*y) + m[0][0]*x) + m[3][0] and, for y and z,
/// ((m[2][a]*z + m[0][a]*x) + m[1][a]*y) + m[3][a]; every input is loaded
/// before the stores, which are z (+8), x (+0), y (+4) (0x2e96f..0x2e976).
fn transform_position(p: [f32; 3], m: &[f32; 16]) -> Result<[f64; 3], ()> {
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
    let y_axis = axis(1)?;
    Ok([x_axis, y_axis, axis(2)?])
}

/// `S_vector::RotateByMatrix(const S_matrix &)`, LS3DF 0x2ea10: each axis
/// `(x*m[0][a] + y*m[1][a]) + z*m[2][a]` in registers, stored to a stack
/// temporary in x, y, z order; no translation. `SetSpace` then copies the
/// temporary into the vertex normal with integer moves (0x37bce..0x37bdd).
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

/// The `S_vector::Normalize` inlined in `SetSpace` (see the module notes)
/// over the normal just stored, issuing its stores through `put` in native
/// order (each store before the next product).
fn normalize_normal_into<E>(
    n: [f32; 3],
    put: &mut impl FnMut(usize, f32) -> Result<(), VertexBakeStop<E>>,
) -> Result<(), VertexBakeStop<E>> {
    let numeric = |()| VertexBakeStop::Numeric;
    let [x, y, z] = [load(n[0]), load(n[1]), load(n[2])].map(|v| v.map_err(numeric));
    let [x, y, z] = [x?, y?, z?];
    let len2 =
        (|| store(fadd(fadd(fmul(x, x)?, fmul(y, y)?)?, fmul(z, z)?)?))().map_err(numeric)?;
    let len2_wide = f64::from(len2);
    if fsub(len2_wide, 1.0).map_err(numeric)?.abs() < f64::from(NORMALIZE_EPSILON) {
        return Ok(());
    }
    let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
    if len2 < NORMALIZE_EPSILON {
        return if n[0] != 0.0 {
            put(0, sign(n[0]))
        } else if n[2] != 0.0 {
            put(2, sign(n[2]))
        } else {
            put(1, sign(n[1]))
        };
    }
    let reciprocal = (|| fdiv(1.0, fsqrt(len2_wide)?))().map_err(numeric)?;
    for (word, value) in [x, y, z].into_iter().enumerate() {
        put(
            word,
            (|| store(fmul(reciprocal, value)?))().map_err(numeric)?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
fn normalize_normal(n: [f32; 3]) -> Result<[f32; 3], ()> {
    let mut out = n;
    normalize_normal_into::<()>(n, &mut |word, value| {
        out[word] = value;
        Ok(())
    })
    .map_err(|_| ())?;
    Ok(out)
}

/// Why one vertex of the bake stopped.
#[derive(Debug, PartialEq)]
pub(crate) enum VertexBakeStop<E> {
    /// A value left the guarded PC24 domain; the stores before it stand.
    Numeric,
    /// The owner refused a store.
    Store(E),
}

/// One vertex of the bake loop, as its stores reach the vertex: the position
/// words z, x, y (0x2e920); the rotated normal x, y, z copied from the stack
/// temporary; then the inlined Normalize's stores, which read the normal
/// just stored. `write(word, value)` stores `I3D_vertex` word 0..2
/// (position) or 3..5 (normal). A stop keeps every earlier store, as the
/// original has no failure path between them.
pub(crate) fn bake_world_vertex<E>(
    position: [f32; 3],
    normal: [f32; 3],
    matrix: &SMatrix,
    mut write: impl FnMut(usize, f32) -> Result<(), E>,
) -> Result<(), VertexBakeStop<E>> {
    let numeric = |()| VertexBakeStop::Numeric;
    let mut put = |word: usize, value: f32| write(word, value).map_err(VertexBakeStop::Store);
    let [x, y, z] = transform_position(position, &matrix.elements).map_err(numeric)?;
    put(2, store(z).map_err(numeric)?)?;
    put(0, store(x).map_err(numeric)?)?;
    put(1, store(y).map_err(numeric)?)?;
    let rotated = rotate_normal(normal, &matrix.elements).map_err(numeric)?;
    for (axis, value) in rotated.into_iter().enumerate() {
        put(3 + axis, value)?;
    }
    normalize_normal_into(rotated, &mut |word, value| put(3 + word, value))
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
    /// before it, as native has no failure path, and marks the visual: a
    /// later call is refused before any effect instead of transforming the
    /// already written vertices again.
    pub fn set_object_space_world(
        &mut self,
        key: FrameKey,
    ) -> Result<ObjectSpaceWorld, ModelError> {
        let fi = self.frame_index(key)?;
        if self.frames[fi].world_space_bake_interrupted.is_some() {
            return Err(ModelError::SetSpaceInterrupted(key));
        }
        let (object, visual) = self.space_visual_object(key)?;
        if !matches!(
            visual,
            ObjectVisualType::Plain | ObjectVisualType::LitUnconstructed
        ) {
            return Err(ModelError::SetSpaceUnsupportedVisual(key));
        }
        if self.state(key)?.visual_flags & VISF_WORLD_SPACE != 0 {
            return Ok(ObjectSpaceWorld::AlreadyWorld);
        }
        let result = self.object_space_world_arm(key, object);
        if let Err(error) = &result {
            self.frames[fi].world_space_bake_interrupted = Some(error.clone());
        }
        result
    }

    fn object_space_world_arm(
        &mut self,
        key: FrameKey,
        object: crate::i3d_object::ObjectResourceKey,
    ) -> Result<ObjectSpaceWorld, ModelError> {
        if self.state(key)?.visual_flags & VISF_WBOUND_VALID == 0 {
            self.update_visual_wbound_proc(key)?;
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
                            bake_world_vertex(position, normal, &matrix, |word, value| {
                                owner.game_write_vertex_word(window, vertex, word, value)
                            })
                            .map_err(|stop| match stop {
                                VertexBakeStop::Numeric => ModelError::SetSpaceNumeric(key),
                                VertexBakeStop::Store(error) => loaded(error),
                            })?;
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
