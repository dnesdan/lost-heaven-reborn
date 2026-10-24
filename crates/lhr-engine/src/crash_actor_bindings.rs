//! Original actor-reference removal reached by temporary actor destruction.
use super::*;
fn rows<E>(begin: u32, end: u32, stride: u32) -> Out<u32, E> {
    end.checked_sub(begin)
        .filter(|n| n.is_multiple_of(stride) && *n / stride <= 1048576)
        .map(|n| n / stride)
        .ok_or(CrashActorError::StorageRange)
}
/// C_PhThing460140 removes every retained contact with the dying actor.
pub fn crash_remove_contact_actor<B: VehicleInitBackend>(
    b: &mut B,
    a: u32,
    target: u32,
) -> Out<(), B::Error> {
    let begin = word(b, a + 0x380)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, a + 0x384)?;
    for i in (0..rows::<B::Error>(begin, end, 20)?).rev() {
        let begin = word(b, a + 0x380)?;
        if word(b, begin + i * 20 + 12)? == target {
            let last = word(b, a + 0x384)? - 20;
            copy(b, last, begin + i * 20, 5)?;
            let end = word(b, a + 0x384)?;
            put(b, a + 0x384, end - 20)?;
        }
    }
    Ok(())
}
fn remove_pointer<B: VehicleInitBackend>(
    b: &mut B,
    begin_field: u32,
    end_field: u32,
    target: u32,
) -> Out<(), B::Error> {
    let begin = word(b, begin_field)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, end_field)?;
    for i in (0..count::<B::Error>(begin, end)?).rev() {
        let begin = word(b, begin_field)?;
        if word(b, begin + i * 4)? != target {
            continue;
        }
        let end = word(b, end_field)?;
        let position = begin + i * 4;
        copy(b, position + 4, position, (end - position) / 4 - 1)?;
        let end = word(b, end_field)?;
        put(b, end_field, end - 4)?;
    }
    Ok(())
}
/// C_game5afe40 actor-reference cleanup. Its empty mission registries are
/// admitted by their real guards. Occupied script-owned records keep their
/// original worker frontier until their nontrivial nested owners are supplied.
pub fn crash_remove_actor_bindings<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
    target: u32,
) -> Out<(), B::Error> {
    if target == 0 {
        return Ok(());
    }
    let begin = word(b, 0x63d2bc)?;
    if begin != 0 {
        let end = word(b, 0x63d2c0)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            let begin = word(b, 0x63d2bc)?;
            let a = word(b, begin + i * 4)?;
            match word(b, a + 0x10)? {
                0x11 | 0x1a | 0x20 | 0x23 | 0x25 => crash_remove_contact_actor(b, a, target)?,
                5 | 0x1b => return Err(CrashActorError::Dependency(0x46cec0)),
                2 => {
                    for offset in [0xa74, 0x98, 0x9c] {
                        if word(b, a + offset)? == target {
                            put(b, a + offset, 0)?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let begin = word(b, game + 0x1d8)?;
    if begin != 0 {
        let end = word(b, game + 0x1dc)?;
        for i in (0..rows::<B::Error>(begin, end, 0x5c)?).rev() {
            let begin = word(b, game + 0x1d8)?;
            if word(b, begin + i * 0x5c + 0x2c)? == target {
                let begin = word(b, game + 0x1d8)?;
                put(b, begin + i * 0x5c + 0x2c, 0)?;
            }
        }
    }
    crash_clear_actor_slots(b, game + 0x2b3c, target)?;
    let target_kind = word(b, target + 0x10)?;
    if target_kind == 4 {
        return Err(CrashActorError::Dependency(0x5b8c20));
    }
    let mut i = 0;
    loop {
        let begin = word(b, game + 0x318)?;
        if begin == 0 {
            break;
        }
        let end = word(b, game + 0x31c)?;
        if i >= count::<B::Error>(begin, end)? {
            break;
        }
        let record = word(b, begin + i * 4)?;
        if word(b, record + 0xc)? == word(b, target + 0x68)? {
            return Err(CrashActorError::Dependency(0x55d8a0));
        }
        i += 1;
    }
    let begin = word(b, 0x646d44 + 0xc)?;
    if begin != 0 && rows::<B::Error>(begin, word(b, 0x646d44 + 0x10)?, 0x60)? != 0 {
        return Err(CrashActorError::Dependency(0x531dc0));
    }
    remove_pointer(b, 0x646d44 + 0x1c, 0x646d44 + 0x20, target)?;
    let begin = word(b, 0x646d80 + 4)?;
    if begin != 0 && rows::<B::Error>(begin, word(b, 0x646d80 + 8)?, 0x48)? != 0 {
        return Err(CrashActorError::Dependency(0x536200));
    }
    let begin = word(b, 0x646d6c + 4)?;
    if begin != 0 && rows::<B::Error>(begin, word(b, 0x646d6c + 8)?, 0x38)? != 0 {
        return Err(CrashActorError::Dependency(0x5331c0));
    }
    put(b, 0x646d6c + 0x10, 0)?;
    crash_empty_script_update(b, 0x646d6c)?;
    if target_kind == 2 || target_kind == 0x1b {
        return Err(CrashActorError::Dependency(0x5f53d0));
    }
    let frame = word(b, target + 0x68)?;
    let begin = word(b, 0x63d2a8)?;
    if begin != 0 {
        let end = word(b, 0x63d2ac)?;
        for i in (0..rows::<B::Error>(begin, end, 8)?).rev() {
            if word(b, begin + i * 8)? == frame {
                let end = word(b, 0x63d2ac)?;
                let first = word(b, end - 8)?;
                put(b, begin + i * 8, first)?;
                let second = word(b, end - 4)?;
                put(b, begin + i * 8 + 4, second)?;
                let end = word(b, 0x63d2ac)?;
                put(b, 0x63d2ac, end - 8)?;
                break;
            }
        }
    }
    if word(b, game + 0xe4)? == target {
        put(b, game + 0xe4, 0)?;
    }
    if word(b, game + 0x204)? == target {
        put(b, game + 0x204, 0)?;
    }
    let begin = word(b, game + 0x368c)?;
    if begin != 0 {
        let end = word(b, game + 0x3690)?;
        for i in (0..rows::<B::Error>(begin, end, 0x28)?).rev() {
            if word(b, begin + i * 0x28)? == target {
                return Err(CrashActorError::Dependency(0x5baa40));
            }
        }
    }
    Ok(())
}

/// Game55fd10 clears matching identities in sixteen source-owned slots.
pub fn crash_clear_actor_slots<B: VehicleInitBackend>(
    b: &mut B,
    manager: u32,
    target: u32,
) -> Out<(), B::Error> {
    for i in 0..16 {
        let slot = manager + 0x1c + i * 0x20;
        if word(b, slot)? == target {
            put(b, slot - 0x10, 0)?;
            put(b, slot, 0)?;
        }
    }
    Ok(())
}
/// Empty-record path of manager5325a0, including its observable timer update.
pub fn crash_empty_script_update<B: VehicleInitBackend>(
    b: &mut B,
    manager: u32,
) -> Out<(), B::Error> {
    let app = word(b, 0x63788c)?;
    let game = word(b, app + 0x24)?;
    let time = word(b, game + 0x2b0c)?;
    if word(b, manager + 0x10)? < time {
        put(b, manager + 0x10, time.wrapping_add(2000))?;
    }
    let begin = word(b, manager + 4)?;
    if begin != 0 && rows::<B::Error>(begin, word(b, manager + 8)?, 0x38)? != 0 {
        return Err(CrashActorError::Dependency(0x5325a0));
    }
    Ok(())
}

/// C_crash_obj432740 marks the furthest live crash actor for removal. The Game
/// invokes this before actor dispatch when its count exceeds twenty.
pub fn crash_retire_furthest_actor<B: CrashActorBackend>(b: &mut B) -> Out<(), B::Error> {
    if word(b, 0x63d6d4)? == 0 {
        return Ok(());
    }
    let app = word(b, 0x63788c)?;
    let scene = word(b, app + 0x10)?;
    let camera = word(b, scene + 0x17c)?;
    world(b, camera)?;
    let p = [
        float(b, camera + 0x40)?,
        float(b, camera + 0x44)?,
        float(b, camera + 0x48)?,
    ];
    let app = word(b, 0x63788c)?;
    let game = word(b, app + 0x24)?;
    let begin = word(b, game + 0x124)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, game + 0x128)?;
    let mut furthest = 0;
    let mut maximum = -1.;
    for i in (0..count::<B::Error>(begin, end)?).rev() {
        let begin = word(b, game + 0x124)?;
        let a = word(b, begin + i * 4)?;
        if word(b, a + 0x10)? != 0x1a {
            continue;
        }
        let f = word(b, a + 0x68)?;
        world(b, f)?;
        let x = sub(p[0], float(b, f + 0x40)?);
        let y = sub(p[1], float(b, f + 0x44)?);
        let z = sub(p[2], float(b, f + 0x48)?);
        let square = add(add(mul(z, z), mul(x, x)), mul(y, y));
        if maximum < square {
            let begin = word(b, game + 0x124)?;
            furthest = word(b, begin + i * 4)?;
            maximum = square;
        }
    }
    if furthest != 0 {
        let flags = word(b, furthest + 0x38c)?;
        put(b, furthest + 0x38c, flags | 0x8000)?;
    }
    Ok(())
}
