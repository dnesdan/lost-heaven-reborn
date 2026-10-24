//! Source actor dispatch and Game temporary-actor list phases.
use super::*;

/// C_crash_obj::AI431be0. Unreconstructed destruction emitters retain their
/// precise reached frontier; the common healthy path runs real PhThing AI.
pub fn crash_actor_ai<B: CrashActorBackend>(
    b: &mut B,
    a: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let flags = word(b, a + 0x38c)?;
    if flags & 0x8000 != 0 {
        call(b, a, 0x5afe40, [0, 0])?;
        let app = word(b, 0x63788c)?;
        let game = word(b, app + 0x24)?;
        return crash_remove_temporary_actor(b, game, a);
    }
    if flags & 1 == 0 || float(b, a + 0x390)? >= 0. {
        return call(b, a, 0x45a690, [milliseconds, 0]).map(|_| ());
    }
    let f = word(b, a + 0x68)?;
    world(b, f)?;
    for (mask, entry) in [
        (8, 0x5abe10),
        (0x20, 0x5aae10),
        (2, 0x562170),
        (0x10, 0x565720),
        (4, 0x565060),
    ] {
        if word(b, a + 0x38c)? & mask != 0 {
            return Err(CrashActorError::Dependency(entry));
        }
    }
    if byte(b, a + 0x38c)? & 0x40 != 0 && word(b, a + 0x39c)? != 0 {
        return Err(CrashActorError::Dependency(0x430c50));
    }
    let app = word(b, 0x63788c)?;
    if word(b, app + 0xa8)? > 1 && byte(b, a + 0x38c)? & 0x80 != 0 {
        return Err(CrashActorError::Dependency(0x449690));
    }
    call(b, a, 0x5afe40, [0, 0])?;
    let app = word(b, 0x63788c)?;
    let game = word(b, app + 0x24)?;
    crash_remove_temporary_actor(b, game, a)
}
/// Original C_actor::Tick4064f0 over canonical fields. Crash's actual Death
/// leaf4aabe0 is empty; its gate writes still occur in their native order.
pub fn crash_actor_tick<B: CrashActorBackend>(
    b: &mut B,
    a: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let model = word(b, a + 0x68)?;
    if model == 0 {
        return Err(CrashActorError::StorageRange);
    }
    let kind = word(b, model + 0x110)?;
    let visible = if kind == 1 || kind == 9 {
        let token = word(b, model + 0x13c)?;
        let app = word(b, 0x63788c)?;
        let scene = word(b, app + 0x10)?;
        u8::from(token == word(b, scene + 0x13c)?)
    } else {
        1
    };
    putb(b, a + 0x5f, visible)?;
    if byte(b, a + 4)? & 5 == 0 {
        if byte(b, a + 0x20)? == 0 {
            crash_actor_ai(b, a, milliseconds)?;
        } else {
            call(b, a, 0x443860, [milliseconds, 0])?;
        }
    }
    call(b, a, 0x45a7a0, [milliseconds, 0])?;
    if byte(b, a + 0x5d)? == 0 && byte(b, a + 0x5e)? == 0 {
        putb(b, a + 0x5e, 1)?;
        putb(b, a + 0x5d, 0)?;
    }
    Ok(())
}

/// C_game helper5a5070 moves state-changed actors between its two buckets.
pub fn crash_migrate_actor_states<B: CrashActorBackend>(b: &mut B, game: u32) -> Out<(), B::Error> {
    putb(b, game + 0x10c, 0)?;
    for (header, target, expected) in [(game + 0xec, game + 0xfc, 0), (game + 0xfc, game + 0xec, 1)]
    {
        let mut i = 0;
        loop {
            let begin = word(b, header + 4)?;
            if begin == 0 {
                break;
            }
            let end = word(b, header + 8)?;
            if i >= count::<B::Error>(begin, end)? {
                break;
            }
            let actor = word(b, begin + i * 4)?;
            if word(b, actor + 0x1c)? == expected {
                i += 1;
                continue;
            }
            crash_actor_change_state(b, actor)?;
            let begin = word(b, header + 4)?;
            let actor = word(b, begin + i * 4)?;
            if word(b, actor + 0x1c)? != 2 {
                let position = word(b, target + 8)?;
                vehicle_component_pointers_insert(b, target, position, 1, begin + i * 4)
                    .map_err(CrashActorError::Backend)?;
            }
            let begin = word(b, header + 4)?;
            let end = word(b, header + 8)?;
            let last = word(b, end - 4)?;
            put(b, begin + i * 4, last)?;
            let end = word(b, header + 8)?;
            put(b, header + 8, end - 4)?;
        }
    }
    Ok(())
}
/// Active actor phase of C_game5a51c0, following the caller's Car wrapper.
/// This standalone owner admits only C_crash_obj pointers in these new lists.
pub fn crash_temporary_actor_phase<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let mut i = 0;
    loop {
        let begin = word(b, game + 0xf0)?;
        if begin == 0 {
            break;
        }
        let end = word(b, game + 0xf4)?;
        if i >= count::<B::Error>(begin, end)? {
            break;
        }
        let a = word(b, begin + i * 4)?;
        if word(b, a)? != 0x623918 {
            return Err(CrashActorError::Dependency(0x4064f0));
        }
        crash_actor_tick(b, a, milliseconds)?;
        i += 1;
    }
    if byte(b, game + 0x10c)? != 0 {
        crash_migrate_actor_states(b, game)?;
    }
    let begin = word(b, game + 0x134)?;
    if begin != 0 {
        let end = word(b, game + 0x138)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            let begin = word(b, game + 0x134)?;
            let a = word(b, begin + i * 4)?;
            crash_delete_queued_actor(b, game, a)?;
        }
        let begin = word(b, game + 0x134)?;
        put(b, game + 0x138, begin)?;
    }
    Ok(())
}
