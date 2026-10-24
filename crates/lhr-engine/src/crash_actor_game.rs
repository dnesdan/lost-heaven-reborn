//! Game registration and explicit teardown of the same C_crash_obj allocation.
use super::*;

/// C_crash_obj::GameInit432460 with C_PhThing45a120. Its collision descriptor,
/// sound and frame remain owned by the caller's existing graph and heap.
pub fn crash_actor_game_init<B: CrashActorBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    call(b, a, 0x405fa0, [0, 0])?;
    phthing_matrices(b, a)?;
    let f = word(b, a + 0x68)?;
    world(b, f)?;
    copy(b, f + 0x40, a + 0x24, 3)?;
    if byte(b, f + 0xac)? & 0x40 == 0 {
        effect(b, CrashActorEffect::UpdateHierarchyBounds { frame: f })?;
    }
    let mut bounds = [0.; 6];
    for (i, n) in bounds.iter_mut().enumerate() {
        *n = float(b, f + 0xb0 + i as u32 * 4)?;
    }
    let square = |v: &[f32]| add(add(mul(v[1], v[1]), mul(v[2], v[2])), mul(v[0], v[0]));
    let low = square(&bounds[..3]);
    let high = square(&bounds[3..]);
    let length = if high > low { high } else { low };
    let radius = if (length.to_bits() as i32) < 0x322bcc77 {
        0.
    } else if f64::from(sub(length, 1.).abs()) < f64::from(1e-8_f32) {
        1.
    } else {
        crate::native_precision::sqrt(length)
    };
    for i in 0..0x12 {
        put(b, a + 0xb8 + i * 4, 0)?;
    }
    putf(b, a + 0xd4, radius)?;
    put(b, a + 0xb8, 0x40)?;
    put(b, a + 0xf8, a + 0x14c)?;
    put(b, a + 0xfc, a)?;
    let x = word(b, a + 0x24)?;
    put(b, a + 0xc8, x)?;
    put(b, a + 0xc0, 0x800)?;
    copy(b, a + 0x28, a + 0xcc, 2)?;
    for (i, n) in bounds.into_iter().enumerate() {
        putf(b, a + 0xdc + i as u32 * 4, n)?;
    }
    let f = word(b, a + 0x68)?;
    put(b, a + 0xbc, f)?;
    put(b, a + 0xf4, a + 0x10c)?;
    call(b, a, 0x5c36c0, [a + 0xb8, 0])?;
    call(b, a, 0x5c3ac0, [a + 0xb8, 0])?;
    if byte(b, a + 0x76)? == 0 {
        copy(b, a + 0x24, a + 0x84, 3)?;
    } else {
        let mut elements = [0.; 16];
        for (i, n) in elements.iter_mut().enumerate() {
            *n = float(b, a + 0x10c + i as u32 * 4)?;
        }
        let point = [
            float(b, a + 0x78)?,
            float(b, a + 0x7c)?,
            float(b, a + 0x80)?,
        ];
        for (i, n) in (SMatrix { elements })
            .transform_point(point)
            .into_iter()
            .enumerate()
        {
            putf(b, a + 0x84 + i as u32 * 4, n)?;
        }
    }
    for o in [0x314, 0x22c, 0x230, 0x234, 0x32c, 0x318, 0x31c, 0x320] {
        put(b, a + o, 0)?;
    }
    let x = word(b, a + 0x24)?;
    put(b, a + 0x90, x)?;
    put(b, a + 0x330, 0x3f19999a)?;
    put(b, a + 0x9c, 0)?;
    copy(b, a + 0x28, a + 0x94, 2)?;
    let sound = effect(b, CrashActorEffect::CreateSound)?;
    put(b, a + 0x338, sound)?;
    if sound != 0 {
        let app = word(b, 0x63788c)?;
        let scene = word(b, app + 0x10)?;
        let primary = word(b, scene + 0x210)?;
        frame(b, sound, 0x2c, &[primary, 0])?;
        if byte(b, sound + 0xac)? & 0x80 == 0 {
            for o in [0x80, 0x84, 0x88] {
                put(b, sound + o, 0)?;
            }
            let flags = word(b, sound + 0xac)?;
            put(b, sound + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
        }
    }
    for i in 0..7 {
        put(b, a + 0x350 + i * 4, 0)?;
    }
    putb(b, a + 0x36c, 0)?;
    putb(b, a + 0x36d, 0)?;
    put(b, a + 0x370, 0)?;
    put(b, a + 0x374, 0)?;
    put(b, a + 0x334, 0x3f800000)?;
    putb(b, a + 0x75, 1)?;
    let v = [
        float(b, a + 0x220)?,
        float(b, a + 0x224)?,
        float(b, a + 0x228)?,
    ];
    if add(add(mul(v[0], v[0]), mul(v[1], v[1])), mul(v[2], v[2])) < 0.01 {
        crash_actor_set_state(b, a, 1)?;
    }
    let n = word(b, 0x63d6d4)?;
    put(b, 0x63d6d4, n.wrapping_add(1))
}

/// C_crash_obj::GameDone432470 and C_PhThing45a630. GameDone does not free the
/// actor allocation; its deleting destructor remains the final Release call.
pub fn crash_actor_game_done<B: CrashActorBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    call(b, a, 0x4063e0, [0, 0])?;
    let sound = word(b, a + 0x338)?;
    if sound != 0 {
        frame(b, sound, 0, &[])?;
    }
    call(b, a, 0x5c38b0, [a + 0xb8, 0])?;
    let begin = word(b, a + 0x380)?;
    put(b, a + 0x384, begin)?;
    let name = word(b, a + 0x39c)?;
    if name != 0 {
        b.free(name).map_err(CrashActorError::Backend)?;
        put(b, a + 0x39c, 0)?;
    }
    let f = word(b, a + 0x68)?;
    frame(b, f, 0x2c, &[0, 0])?;
    let f = word(b, a + 0x68)?;
    frame(b, f, 0, &[])?;
    let n = word(b, 0x63d6d4)?;
    put(b, 0x63d6d4, n.wrapping_sub(1))
}

/// C_game::RemoveTemporaryActor5a79a0 queues deletion once and marks state2.
pub fn crash_remove_temporary_actor<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
    a: u32,
) -> Out<(), B::Error> {
    let begin = word(b, game + 0x134)?;
    if begin != 0 {
        let end = word(b, game + 0x138)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            if word(b, begin + i * 4)? == a {
                return Ok(());
            }
        }
    }
    crash_actor_set_state(b, a, 2)?;
    append(b, game + 0x130, a)
}
