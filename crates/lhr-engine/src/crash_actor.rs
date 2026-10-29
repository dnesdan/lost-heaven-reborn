//! C_crash_obj/C_PhThing construction on the caller's canonical address space.
//! Original PC entries are retained beside each worker. Frame and mission
//! calls remain ordered effects on the same owner as the actor allocation.
use crate::native_precision::{add, div, mul, sub};
use crate::native_transform::SMatrix;
use crate::vehicle_component_records::vehicle_component_pointers_insert;
use crate::vehicle_init::VehicleInitBackend;
#[path = "crash_actor_game.rs"]
mod game;
pub use game::*;
#[path = "crash_actor_cleanup.rs"]
mod cleanup;
pub use cleanup::*;
#[path = "crash_actor_dispatch.rs"]
mod dispatch;
pub use dispatch::*;
#[path = "crash_actor_bindings.rs"]
mod bindings;
pub use bindings::*;
#[cfg(test)]
#[path = "crash_actor_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashActorEffect {
    CreateSound,
    SetFrameSector {
        frame: u32,
        position: u32,
    },
    FrameVirtual {
        frame: u32,
        slot: u32,
        arguments: [u32; 3],
        count: u8,
    },
    FindFrameNamed {
        root: u32,
        name: &'static [u8],
        flags: u32,
    },
    UpdateWorld {
        frame: u32,
    },
    UpdateHierarchyBounds {
        frame: u32,
    },
    Dependency {
        actor: u32,
        entry: u32,
        arguments: [u32; 2],
    },
}
pub trait CrashActorBackend: VehicleInitBackend {
    fn crash_actor_effect(&mut self, effect: CrashActorEffect) -> Result<u32, Self::Error>;
}
#[derive(Debug, PartialEq, Eq)]
pub enum CrashActorError<E> {
    Backend(E),
    StorageRange,
    NumericDomain(&'static str),
    AllocationFailed,
    Dependency(u32),
}
impl<E: From<crate::vehicle_init::VehicleUninitializedStorage>>
    From<crate::vehicle_init::VehicleUninitializedStorage> for CrashActorError<E>
{
    fn from(error: crate::vehicle_init::VehicleUninitializedStorage) -> Self {
        Self::Backend(E::from(error))
    }
}

type Out<T, E> = Result<T, CrashActorError<E>>;
fn word<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(CrashActorError::Backend)
}
fn byte<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(CrashActorError::Backend)
}
fn put<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v).map_err(CrashActorError::Backend)
}
fn putb<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u8) -> Out<(), B::Error> {
    b.write_byte(a, v).map_err(CrashActorError::Backend)
}
fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(word(b, a)?))
}
fn putf<B: VehicleInitBackend>(b: &mut B, a: u32, v: f32) -> Out<(), B::Error> {
    put(b, a, v.to_bits())
}
fn copy<B: VehicleInitBackend>(b: &mut B, src: u32, dst: u32, n: u32) -> Out<(), B::Error> {
    for i in 0..n {
        let v = word(b, src + i * 4)?;
        put(b, dst + i * 4, v)?;
    }
    Ok(())
}
fn effect<B: CrashActorBackend>(b: &mut B, e: CrashActorEffect) -> Out<u32, B::Error> {
    b.crash_actor_effect(e).map_err(CrashActorError::Backend)
}
fn call<B: CrashActorBackend>(
    b: &mut B,
    actor: u32,
    entry: u32,
    arguments: [u32; 2],
) -> Out<u32, B::Error> {
    effect(
        b,
        CrashActorEffect::Dependency {
            actor,
            entry,
            arguments,
        },
    )
}
fn frame<B: CrashActorBackend>(b: &mut B, f: u32, slot: u32, args: &[u32]) -> Out<u32, B::Error> {
    let mut arguments = [0; 3];
    arguments[..args.len()].copy_from_slice(args);
    effect(
        b,
        CrashActorEffect::FrameVirtual {
            frame: f,
            slot,
            arguments,
            count: args.len() as u8,
        },
    )
}
fn world<B: CrashActorBackend>(b: &mut B, f: u32) -> Out<(), B::Error> {
    if byte(b, f + 0xac)? & 0x20 == 0 {
        effect(b, CrashActorEffect::UpdateWorld { frame: f })?;
    }
    Ok(())
}
fn count<E>(begin: u32, end: u32) -> Out<u32, E> {
    end.checked_sub(begin)
        .filter(|n| n.is_multiple_of(4) && *n <= 4 * 1048576)
        .map(|n| n / 4)
        .ok_or(CrashActorError::StorageRange)
}

// Native pointer-vector append for a nonaliasing scalar caller argument. The
// vector header includes its one-byte allocator marker before begin/end/cap.
fn append<B: VehicleInitBackend>(b: &mut B, header: u32, value: u32) -> Out<(), B::Error> {
    let end = word(b, header + 8)?;
    let cap = word(b, header + 12)?;
    if count::<B::Error>(end, cap)? != 0 {
        if end != 0 {
            put(b, end, value)?;
        }
        let end = word(b, header + 8)?;
        return put(b, header + 8, end.wrapping_add(4));
    }
    let begin = word(b, header + 4)?;
    let len = if begin == 0 {
        0
    } else {
        count::<B::Error>(begin, end)?
    };
    let capacity = len
        .checked_add(len.max(1))
        .filter(|n| *n <= 1048576)
        .ok_or(CrashActorError::StorageRange)?;
    let fresh = b.allocate(capacity * 4).map_err(CrashActorError::Backend)?;
    if fresh == 0 {
        return Err(CrashActorError::AllocationFailed);
    }
    let begin = word(b, header + 4)?;
    copy(b, begin, fresh, len)?;
    put(b, fresh + len * 4, value)?;
    let current_end = word(b, header + 8)?;
    let suffix = count::<B::Error>(end, current_end)?;
    copy(b, end, fresh + len * 4 + 4, suffix)?;
    let old = word(b, header + 4)?;
    b.free(old).map_err(CrashActorError::Backend)?;
    put(b, header + 12, fresh + capacity * 4)?;
    let begin = word(b, header + 4)?;
    let len = if begin == 0 {
        0
    } else {
        let end = word(b, header + 8)?;
        count::<B::Error>(begin, end)?
    };
    put(b, header + 4, fresh)?;
    put(b, header + 8, fresh + len * 4 + 4)
}
fn actor_construct<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    put(b, a + 4, 0)?;
    putb(b, a + 0xc, 1)?;
    putb(b, a + 0xd, 0)?;
    put(b, a + 0x10, 1)?;
    put(b, a + 0x14, 1)?;
    putb(b, a + 0x20, 0)?;
    put(b, a + 0x60, 0)?;
    putb(b, a + 0x66, 0)?;
    put(b, a + 0x68, 0)?;
    put(b, a, 0x623248)?;
    put(b, a + 8, 0)?;
    let n = word(b, 0x63d2c8)?;
    put(b, 0x63d2c8, n.wrapping_add(1))?;
    let begin = word(b, 0x63d2bc)?;
    if begin != 0 {
        let end = word(b, 0x63d2c0)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            if word(b, begin + i * 4)? == a {
                return Ok(());
            }
        }
    }
    append(b, 0x63d2b8, a)
}
/// C_crash_obj constructor431b40, including base C_PhThing459e90/C_actor405d20.
/// `marker` is the untouched byte in the base constructor's caller stack.
pub fn crash_actor_construct<B: VehicleInitBackend>(
    b: &mut B,
    a: u32,
    marker: u8,
) -> Out<u32, B::Error> {
    actor_construct(b, a)?;
    for i in 0..0x12 {
        put(b, a + 0xb8 + i * 4, 0)?;
    }
    putb(b, a + 0x37c, marker)?;
    for (o, v) in [
        (0x380, 0),
        (0x384, 0),
        (0x388, 0),
        (0, 0x624088),
        (0x70, 0),
        (0x25c, 0),
        (0x260, 0),
        (0x254, 0x41200000),
        (0x20c, 0x3ce56042),
        (0x220, 0),
        (0x224, 0),
        (0x228, 0),
        (0x22c, 0),
        (0x230, 0),
        (0x234, 0),
        (0x100, 0),
        (0x104, 0x3f800000),
        (0x108, 0),
        (0x348, u32::MAX),
        (0x34c, u32::MAX),
        (0x78, 0),
        (0x7c, 0),
        (0x80, 0),
    ] {
        put(b, a + o, v)?;
    }
    putb(b, a + 0x76, 0)?;
    putb(b, a + 0x378, 1)?;
    put(b, a + 0x394, 0x41200000)?;
    putb(b, a + 0x398, 0)?;
    put(b, a + 0x39c, 0)?;
    putb(b, a + 0x3bc, 0)?;
    putb(b, a + 0x3bd, 0)?;
    put(b, a, 0x623918)?;
    put(b, a + 0x10, 0x1a)?;
    for o in [0x3ac, 0x3b0, 0x3b4, 0x3a0, 0x3a4, 0x3a8, 0x3b8] {
        put(b, a + o, 0)?;
    }
    Ok(a)
}

fn cstring<B: VehicleInitBackend>(b: &mut B, p: u32) -> Out<Vec<u8>, B::Error> {
    let mut s = Vec::new();
    for i in 0..65536 {
        let c = byte(b, p + i)?;
        s.push(c);
        if c == 0 {
            return Ok(s);
        }
    }
    Err(CrashActorError::StorageRange)
}
/// C_crash_obj::SetModelFileName4326d0. The flag controls ownership exactly;
/// clearing it still releases the previous owned filename.
pub fn crash_actor_set_model_filename<B: VehicleInitBackend>(
    b: &mut B,
    a: u32,
    name: u32,
) -> Out<(), B::Error> {
    let old = word(b, a + 0x39c)?;
    if old != 0 {
        b.free(old).map_err(CrashActorError::Backend)?;
        put(b, a + 0x39c, 0)?;
    }
    if name != 0 && byte(b, a + 0x38c)? & 0x40 != 0 {
        let name = cstring(b, name)?;
        let p = b
            .allocate(name.len() as u32)
            .map_err(CrashActorError::Backend)?;
        if p == 0 {
            return Err(CrashActorError::AllocationFailed);
        }
        put(b, a + 0x39c, p)?;
        for (i, c) in name.into_iter().enumerate() {
            putb(b, p + i as u32, c)?;
        }
    }
    Ok(())
}

/// C_PhThing::InitBox45ee90. The five original words are mass, velocity XYZ
/// and coefficient. The preceding three caller locals are not parameters.
pub fn crash_actor_init_box<B: CrashActorBackend>(
    b: &mut B,
    a: u32,
    p: [u32; 5],
) -> Out<(), B::Error> {
    put(b, a + 0x210, p[0])?;
    put(b, a + 0x238, p[4])?;
    for i in 0..3 {
        put(b, a + 0x220 + i * 4, p[i as usize + 1])?;
    }
    let v = [
        f32::from_bits(p[1]),
        f32::from_bits(p[2]),
        f32::from_bits(p[3]),
    ];
    if add(add(mul(v[0], v[0]), mul(v[1], v[1])), mul(v[2], v[2])) < f32::from_bits(0x3727c5ac) {
        crash_actor_set_state(b, a, 1)?;
    }
    let f = word(b, a + 0x68)?;
    if byte(b, f + 0xac)? & 0x40 == 0 {
        effect(b, CrashActorEffect::UpdateHierarchyBounds { frame: f })?;
    }
    let mut lo = [0.; 3];
    let mut hi = [0.; 3];
    for i in 0..3 {
        lo[i] = float(b, f + 0xb0 + i as u32 * 4)?;
        hi[i] = float(b, f + 0xbc + i as u32 * 4)?;
    }
    let size = std::array::from_fn::<_, 3, _>(|i| sub(hi[i], lo[i]));
    let center = std::array::from_fn::<_, 3, _>(|i| add(mul(size[i], 0.5), lo[i]));
    putb(b, a + 0x76, 1)?;
    for (i, value) in center.into_iter().enumerate() {
        putf(b, a + 0x78 + i as u32 * 4, value)?;
    }
    let [x, y, z] = size.map(|n| add(n, f32::from_bits(0x3d4ccccd)));
    let mass = float(b, a + 0x210)?;
    let third = f32::from_bits(0x3eaaaaab);
    let inverse_y = div(1., y);
    let xy = mul(inverse_y, x);
    let xz = div(x, z);
    let zy = mul(inverse_y, z);
    let inertia = if xy > 2. || xy < 0.5 || xz > 2. || xz < 0.5 || zy > 2. || zy < 0.5 {
        if x > y && x > z {
            let square = mul(x, x);
            let q = div(1., mul(mul(square, mass), third));
            [mul(div(square, mul(y.max(z), y.max(z))), q), q, q]
        } else if x <= y && y > z {
            let square = mul(y, y);
            let q = div(1., mul(mul(square, mass), third));
            [q, mul(div(square, mul(x.max(z), x.max(z))), q), q]
        } else {
            let square = mul(z, z);
            let q = div(1., mul(mul(square, mass), third));
            [q, q, mul(div(square, mul(x.max(y), x.max(y))), q)]
        }
    } else {
        let mean = mul(add(add(z, y), x), third);
        let q = div(1., mul(mul(mul(mean, mean), mass), third));
        [q; 3]
    };
    if !inertia.iter().all(|v| v.is_finite()) {
        return Err(CrashActorError::NumericDomain("InitBox inertia"));
    }
    for (i, v) in inertia.into_iter().enumerate() {
        putf(b, a + 0x214 + i as u32 * 4, v)?;
    }
    let corners = [
        [lo[0], lo[1], lo[2]],
        [lo[0], lo[1], hi[2]],
        [hi[0], lo[1], hi[2]],
        [hi[0], lo[1], lo[2]],
        [hi[0], hi[1], hi[2]],
        [lo[0], hi[1], lo[2]],
        [lo[0], hi[1], hi[2]],
        [hi[0], hi[1], lo[2]],
        [center[0], center[1], lo[2]],
        [center[0], center[1], hi[2]],
        [center[0], lo[1], center[2]],
        [center[0], hi[1], center[2]],
        [lo[0], center[1], center[2]],
        [hi[0], center[1], center[2]],
    ];
    for (i, p) in corners.into_iter().enumerate() {
        for (j, v) in p.into_iter().enumerate() {
            putf(b, a + 0x268 + i as u32 * 12 + j as u32 * 4, v)?;
        }
    }
    let gravity = float(b, a + 0x254)?;
    let zero = mul(mass, 0.);
    putf(b, a + 0x248, mul(zero, gravity))?;
    put(b, a + 0x23c, 0x3f8147ae)?;
    put(b, a + 0x240, 0x3d23d70a)?;
    putf(b, a + 0x24c, mul(mul(mass, -1.), gravity))?;
    put(b, a + 0x70, 2)?;
    putf(b, a + 0x250, mul(zero, gravity))?;
    putf(b, a + 0x244, mul(mul(gravity, mass), 12.5))?;
    putf(b, a + 0x258, mul(mass, 4.))?;
    Ok(())
}

/// C_game::AddTemporaryActor5a77c0. The scratch word is the original caller
/// argument slot used by pointer-vector insertion, never a second actor owner.
pub fn crash_add_temporary_actor<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
    a: u32,
    scratch: u32,
) -> Out<(), B::Error> {
    let mut game_initialized = false;
    crash_add_temporary_actor_with_progress(b, game, a, scratch, &mut game_initialized)
}
/// Same worker, exposing completed GameInit to the caller's ownership journal.
/// A false result after an error does not imply GameInit never started.
pub fn crash_add_temporary_actor_with_progress<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
    a: u32,
    scratch: u32,
    game_initialized: &mut bool,
) -> Out<(), B::Error> {
    *game_initialized = false;
    let begin = word(b, game + 0x124)?;
    if begin != 0 {
        let end = word(b, game + 0x128)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            if word(b, begin + i * 4)? == a {
                return Ok(());
            }
        }
    }
    crash_actor_game_init(b, a)?;
    *game_initialized = true;
    let selector = word(b, a + 0x1c)?;
    if selector <= 1 {
        let header = game + if selector == 0 { 0xec } else { 0xfc };
        let end = word(b, header + 8)?;
        put(b, scratch, a)?;
        vehicle_component_pointers_insert(b, header, end, 1, scratch)
            .map_err(CrashActorError::Backend)?;
    }
    append(b, game + 0x120, a)
}

fn phthing_matrices<B: CrashActorBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    let f = word(b, a + 0x68)?;
    world(b, f)?;
    copy(b, f + 0x10, a + 0x10c, 16)?;
    copy(b, f + 0x10, a + 0x14c, 16)?;
    let mut elements = [0.; 16];
    for (i, v) in elements.iter_mut().enumerate() {
        *v = float(b, a + 0x14c + i as u32 * 4)?;
    }
    let (inverse, _) = SMatrix { elements }
        .inverse()
        .map_err(|_| CrashActorError::NumericDomain("PhThing matrix inverse"))?;
    for (i, v) in inverse.elements.into_iter().enumerate() {
        putf(b, a + 0x14c + i as u32 * 4, v)?;
    }
    copy(b, a + 0x10c, a + 0x18c, 16)?;
    for o in [0x1bc, 0x1c0, 0x1c4] {
        put(b, a + o, 0)?;
    }
    copy(b, a + 0x14c, a + 0x1cc, 16)?;
    for o in [0x1fc, 0x200, 0x204] {
        put(b, a + o, 0)?;
    }
    for (src, dst) in [(0x10c, 0xa0), (0x11c, 0xac), (0x12c, 0x30)] {
        copy(b, a + src, a + dst, 3)?;
    }
    Ok(())
}
fn actor_initialize<B: CrashActorBackend>(b: &mut B, a: u32, f: u32) -> Out<bool, B::Error> {
    if f == 0 {
        return Ok(false);
    }
    let data = word(b, f + 8)?;
    if data != 0 {
        let owner = word(b, data)?;
        if owner != 0 && owner != a {
            return Ok(false);
        }
    }
    put(b, a + 0x68, f)?;
    put(b, f + 8, 0)?;
    let app = word(b, 0x63788c)?;
    let serial = word(b, app + 8)?.wrapping_add(1);
    put(b, app + 8, serial)?;
    put(b, a + 0x18, serial)?;
    putb(b, a + 0x6c, 0)?;
    for name in [b"targetE".as_slice(), b"targetN".as_slice()] {
        let root = word(b, a + 0x68)?;
        let target = effect(
            b,
            CrashActorEffect::FindFrameNamed {
                root,
                name,
                flags: 0x40,
            },
        )?;
        if target != 0 {
            frame(b, target, 0x24, &[0])?;
        }
    }
    put(b, a + 0x1c, 0)?;
    Ok(true)
}
/// C_crash_obj::Init4320f0 including C_PhThing::Init459fd0 and C_actor405dd0.
pub fn crash_actor_initialize<B: CrashActorBackend>(
    b: &mut B,
    a: u32,
    f: u32,
) -> Out<bool, B::Error> {
    if !actor_initialize(b, a, f)? {
        return Ok(false);
    }
    let f = word(b, a + 0x68)?;
    world(b, f)?;
    copy(b, f + 0x40, a + 0x24, 3)?;
    phthing_matrices(b, a)?;
    let f = word(b, a + 0x68)?;
    call(b, f, 0x407cd0, [0, 0])?;
    put(b, a + 0x38c, 0)?;
    put(b, a + 0x390, 0)?;
    putb(b, a + 0x3be, 1)?;
    let property = word(b, f + 0xc)?;
    let text = if property == 0 {
        vec![0]
    } else {
        cstring(b, property)?
    };
    let owned = b
        .allocate(text.len() as u32)
        .map_err(CrashActorError::Backend)?;
    if owned == 0 {
        return Err(CrashActorError::AllocationFailed);
    }
    // The copied property is source-local scratch ownership. Host
    // unavailability retains actor/frame writes, then releases this scratch.
    let parsed: Out<(), B::Error> = (|| {
        for (i, c) in text.iter().copied().enumerate() {
            putb(b, owned + i as u32, c)?;
        }
        if let Some(first) = text.iter().position(|&c| c == b';') {
            let end = text[first + 1..]
                .iter()
                .position(|&c| c == b';')
                .map(|i| first + 1 + i)
                .unwrap_or(text.len() - 1);
            let mut properties = text[first..end].to_vec();
            for (i, c) in properties.iter_mut().enumerate() {
                *c = c.to_ascii_uppercase();
                putb(b, owned + first as u32 + i as u32, *c)?;
            }
            putb(b, owned + end as u32, 0)?;
            for (letter, flag) in [
                (b'G', 2),
                (b'D', 4),
                (b'F', 8),
                (b'S', 0x10),
                (b'E', 0x20),
                (b'+', 0x40),
            ] {
                if properties.contains(&letter) {
                    let flags = word(b, a + 0x38c)?;
                    put(b, a + 0x38c, flags | flag)?;
                }
            }
            if properties.contains(&b'H') {
                call(b, a, 0x5b9b90, [f, 0])?;
            }
            if properties.contains(&b'Z') {
                putb(b, a + 0x3be, 0)?;
            }
            for (letter, offset, flag, as_float, as_byte) in [
                (b'X', 0x390, 1, true, false),
                (b'M', 0x394, 0, true, false),
                (b'N', 0x3bd, 0, false, true),
                (b'L', 0x3bc, 0x80, false, true),
                (b'I', 0x348, 0, false, false),
                (b'V', 0x34c, 0, false, false),
                (b'T', 0x378, 0, false, true),
            ] {
                let Some(index) = properties.iter().position(|&c| c == letter) else {
                    continue;
                };
                if flag != 0 {
                    let flags = word(b, a + 0x38c)?;
                    put(b, a + 0x38c, flags | flag)?;
                }
                let start = index + 1;
                let end = properties[start..]
                    .iter()
                    .position(|c| !c.is_ascii_digit())
                    .map(|n| start + n)
                    .unwrap_or(properties.len());
                let integer = properties[start..end].iter().fold(0u32, |n, c| {
                    n.wrapping_mul(10).wrapping_add(u32::from(*c - b'0'))
                });
                let value = if as_float {
                    let digits = std::str::from_utf8(&properties[start..end])
                        .map_err(|_| CrashActorError::StorageRange)?;
                    let number = if digits.is_empty() {
                        0.
                    } else {
                        digits
                            .parse::<f64>()
                            .map_err(|_| CrashActorError::NumericDomain("Crash property float"))?
                    };
                    (number as f32).to_bits()
                } else {
                    integer
                };
                if as_byte {
                    putb(
                        b,
                        a + offset,
                        if letter == b'T' {
                            u8::from(integer != 0)
                        } else {
                            value as u8
                        },
                    )?;
                } else {
                    put(b, a + offset, value)?;
                }
            }
        }
        Ok(())
    })();
    let released = b.free(owned).map_err(CrashActorError::Backend);
    parsed?;
    released?;
    let f = word(b, a + 0x68)?;
    frame(b, f, 0x20, &[0])?;
    Ok(true)
}

/// The outer crash worker's45a560 final physics activation, not Release.
pub fn crash_actor_kick<B: CrashActorBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    let v = [
        float(b, a + 0x220)?,
        float(b, a + 0x224)?,
        float(b, a + 0x228)?,
    ];
    if add(add(mul(v[0], v[0]), mul(v[1], v[1])), mul(v[2], v[2])) < 0.01 {
        if byte(b, a + 0x20)? == 0 {
            put(b, a + 0x32c, 0)?;
            put(b, a + 0x9c, 0)?;
            call(b, a, 0x431be0, [1, 0])?;
            call(b, a, 0x45a7a0, [1, 0])?;
        }
        let begin = word(b, a + 0x380)?;
        if begin != 0 {
            let end = word(b, a + 0x384)?;
            if end != begin {
                let end_sleep = add(float(b, a + 0x330)?, 1.);
                put(b, a + 0x9c, 0xfa1)?;
                putb(b, a + 0x75, 0)?;
                putf(b, a + 0x32c, end_sleep)?;
                crash_actor_set_state(b, a, 0)?;
                return Ok(());
            }
        }
        crash_actor_set_state(b, a, 1)?;
    }
    Ok(())
}

/// C_actor::Release4091d0 decrements its WORD reference count. The deleting
/// virtual destructor is reached only when that count becomes zero.
pub fn crash_actor_release<B: CrashActorBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    let count = u16::from_le_bytes([byte(b, a + 0xc)?, byte(b, a + 0xd)?]).wrapping_sub(1);
    let bytes = count.to_le_bytes();
    putb(b, a + 0xc, bytes[0])?;
    putb(b, a + 0xd, bytes[1])?;
    if count == 0 {
        crash_actor_destruct(b, a, true)?;
    }
    Ok(())
}
