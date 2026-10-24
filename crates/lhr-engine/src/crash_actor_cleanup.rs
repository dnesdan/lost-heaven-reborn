//! Native state changes and ordered destruction of temporary C_crash_obj actors.
use super::*;
use crate::vehicle_init::{
    VehicleDestructorBackend, VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery,
    VehicleInitUnavailable,
};

/// C_actor::SetState406da0. State2 is terminal; changes dirty the owning Game.
pub fn crash_actor_set_state<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    a: u32,
    state: u32,
) -> Out<(), B::Error> {
    let old = word(b, a + 0x1c)?;
    if old != 2 && old != state {
        let app = word(b, 0x63788c)?;
        let game = word(b, app + 0x24)?;
        putb(b, game + 0x10c, 1)?;
        put(b, a + 0x1c, state)?;
    }
    Ok(())
}
/// C_PhThing::ChangeState45adc0, invoked during the Game's bucket migration.
pub fn crash_actor_change_state<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    if word(b, a + 0x1c)? == 0 {
        if byte(b, a + 0x75)? != 0 {
            put(b, a + 0x32c, 0)?;
            put(b, a + 0x9c, 0)?;
        }
        putb(b, a + 0x75, 1)?;
    }
    Ok(())
}

struct Destruction<'a, B>(&'a mut B);
impl<B: CrashActorBackend> VehicleInitBackend for Destruction<'_, B> {
    type Error = CrashActorError<B::Error>;
    fn read_byte(&mut self, a: u32) -> Out<u8, B::Error> {
        byte(self.0, a)
    }
    fn read_word(&mut self, a: u32) -> Out<u32, B::Error> {
        word(self.0, a)
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Out<(), B::Error> {
        putb(self.0, a, v)
    }
    fn write_word(&mut self, a: u32, v: u32) -> Out<(), B::Error> {
        put(self.0, a, v)
    }
    fn allocate(&mut self, n: u32) -> Out<u32, B::Error> {
        self.0.allocate(n).map_err(CrashActorError::Backend)
    }
    fn free(&mut self, a: u32) -> Out<(), B::Error> {
        self.0.free(a).map_err(CrashActorError::Backend)
    }
    fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, f: u32) -> Out<(), B::Error> {
        self.0.collect_frame(v, f).map_err(CrashActorError::Backend)
    }
    fn enumerate(
        &mut self,
        m: u32,
        q: VehicleInitQuery,
        t: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.0.enumerate(m, q, t).map_err(|e| match e {
            VehicleInitUnavailable::Storage(a) => VehicleInitUnavailable::Storage(a),
            VehicleInitUnavailable::Effect(e) => {
                VehicleInitUnavailable::Effect(CrashActorError::Backend(e))
            }
            VehicleInitUnavailable::AlreadyStarted => VehicleInitUnavailable::AlreadyStarted,
            VehicleInitUnavailable::ProjectionBackendUnavailable => {
                VehicleInitUnavailable::ProjectionBackendUnavailable
            }
        })
    }
}
impl<B: CrashActorBackend> VehicleDestructorBackend for Destruction<'_, B> {
    fn detach_vehicle_frame(&mut self, f: u32) -> Out<(), B::Error> {
        frame(self.0, f, 0x2c, &[0, 0]).map(|_| ())
    }
    fn release_vehicle_object(&mut self, f: u32) -> Out<(), B::Error> {
        frame(self.0, f, 0, &[]).map(|_| ())
    }
}
/// C_crash_obj deleting destructor431bb0/431bd0 and C_PhThing459f90.
/// Explicit GameDone must precede this destructor when GameInit succeeded.
pub fn crash_actor_destruct<B: CrashActorBackend>(
    b: &mut B,
    a: u32,
    delete: bool,
) -> Out<(), B::Error> {
    put(b, a, 0x623918)?;
    put(b, a, 0x624088)?;
    let allocation = word(b, a + 0x380)?;
    b.free(allocation).map_err(CrashActorError::Backend)?;
    for o in [0x380, 0x384, 0x388] {
        put(b, a + o, 0)?;
    }
    crate::vehicle_init::actor_destruct(&mut Destruction(b), a)?;
    if delete {
        b.free(a).map_err(CrashActorError::Backend)?;
    }
    Ok(())
}
/// C_game::ClearTemporaryActors5a7f60, excluding the original two globals.
pub fn crash_clear_temporary_actors<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
) -> Out<(), B::Error> {
    let begin = word(b, game + 0x124)?;
    if begin != 0 {
        let end = word(b, game + 0x128)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            let begin = word(b, game + 0x124)?;
            let a = word(b, begin + i * 4)?;
            if a == word(b, 0x647f28)? || a == word(b, 0x647f2c)? {
                continue;
            }
            call(b, a, 0x5afe40, [0, 0])?;
            let begin = word(b, game + 0x124)?;
            let a = word(b, begin + i * 4)?;
            crash_actor_game_done(b, a)?;
            let begin = word(b, game + 0x124)?;
            let a = word(b, begin + i * 4)?;
            crash_actor_release(b, a)?;
        }
    }
    let begin = word(b, game + 0x124)?;
    put(b, game + 0x128, begin)
}
/// Native5a7ec0 removes one owned temporary pointer, then Done and Release.
pub fn crash_delete_queued_actor<B: CrashActorBackend>(
    b: &mut B,
    game: u32,
    a: u32,
) -> Out<(), B::Error> {
    let begin = word(b, game + 0x124)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, game + 0x128)?;
    for i in (0..count::<B::Error>(begin, end)?).rev() {
        if word(b, begin + i * 4)? != a {
            continue;
        }
        let last = word(b, end - 4)?;
        put(b, begin + i * 4, last)?;
        let end = word(b, game + 0x128)?;
        put(b, game + 0x128, end - 4)?;
        call(b, a, 0x5afe40, [0, 0])?;
        crash_actor_game_done(b, a)?;
        crash_actor_release(b, a)?;
        break;
    }
    Ok(())
}

/// Source5a7d40 detaches temporary identity lists without Done or Release.
/// The caller retains its actual actor reference and chooses its next phase.
pub fn crash_unlink_temporary_actor<B: VehicleInitBackend>(
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
    for header in [game + 0x120, game + 0xec, game + 0xfc] {
        let begin = word(b, header + 4)?;
        if begin == 0 {
            continue;
        }
        let end = word(b, header + 8)?;
        for i in (0..count::<B::Error>(begin, end)?).rev() {
            if word(b, begin + i * 4)? == a {
                let last = word(b, end - 4)?;
                put(b, begin + i * 4, last)?;
                let end = word(b, header + 8)?;
                put(b, header + 8, end - 4)?;
                break;
            }
        }
    }
    Ok(())
}
