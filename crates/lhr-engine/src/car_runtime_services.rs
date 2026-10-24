//! Bounded original game services for the standalone caller context.
use super::*;
use crate::vehicle_frame_backend::{VehicleFrameDamageNotification, VehicleFrameError};
type Error = VehicleFrameError<VehicleMemoryError>;
type Backend<'a> = dyn CarGameInitBackend<Error = Error> + 'a;
impl VehicleFrameGameHost<VehicleMemoryError> for CarRuntimeServices {
    fn audio(&self) -> Option<RuntimeAudio> {
        Some(self.audio.clone())
    }
    fn damage_notification(
        &mut self,
        b: &mut Backend<'_>,
        mut m: VehicleComponentTailMemory<'_>,
        e: VehicleFrameDamageNotification,
    ) -> Result<(), Error> {
        if e.function != 0x426290 {
            return Err(Error::UnavailableGameDependency(e.function));
        }
        // Deform_Vehicle_Part forwards its first three stack arguments plus
        // ECX/EDX to C_game::ScratchAdd; its fourth argument is discarded.
        let app = m.read_word_with_backend(b, 0x63788c)?;
        let game = m.read_word_with_backend(b, app + 0x24)?;
        if e.context == 0 || e.frame == 0 {
            return Ok(());
        }
        if m.read_word_with_backend(b, e.frame + 0x110)? != 1 {
            return Ok(());
        }
        if m.read_word_with_backend(b, e.frame + 0x1d4)? != 0x5f4a424f {
            return Ok(());
        }
        if (2..=5).contains(&e.part_type) || e.part_type == 7 {
            return Ok(());
        }
        // ScratchGetFreeIdx scans the same owned 100-record registry. A
        // projection pool must be reconstructed before occupied slots can
        // run their frame/texture recompilation dependencies.
        for i in (0..100).rev() {
            let record = game + 0x2fd8 + i * 16;
            if m.read_word_with_backend(b, record + 4)? == e.context
                && m.read_word_with_backend(b, record + 8)? == e.frame
            {
                return Err(Error::UnavailableGameEffect(
                    "ScratchGetFreeIdx occupied projection".into(),
                ));
            }
        }
        for i in (0..100).rev() {
            let record = game + 0x2fd8 + i * 16;
            if m.read_word_with_backend(b, record + 4)? != 0 {
                continue;
            }
            m.write_word_with_backend(b, record + 12, 0)?;
            if m.read_word_with_backend(b, record)? == 0 {
                return Ok(());
            }
            return Err(Error::UnavailableGameEffect(
                "ScratchAdd projection compilation".into(),
            ));
        }
        Err(Error::UnavailableGameEffect(
            "ScratchGetFreeIdx full projection pool".into(),
        ))
    }
    fn init_effect(
        &mut self,
        b: &mut Backend<'_>,
        mut m: VehicleComponentTailMemory<'_>,
        e: CarGameInitEffect,
    ) -> Result<u32, Error> {
        match e {
            CarGameInitEffect::FlagsCallback {
                function: 0x4262c0,
                context,
            } => {
                let app = m.read_word_with_backend(b, 0x63788c)?;
                let game = m.read_word_with_backend(b, app + 0x24)?;
                // PC 5b8c20: the owner and frame are reloaded at their
                // original uses, including after LinkTo changes the graph.
                for i in (0..100).rev() {
                    let record = game + 0x2fd8 + i * 16;
                    if m.read_word_with_backend(b, record + 4)? != context {
                        continue;
                    }
                    let frame = m.read_word_with_backend(b, record)?;
                    if frame == 0 {
                        continue;
                    }
                    b.game_init_effect(
                        m.reborrow(),
                        CarGameInitEffect::LinkFrame {
                            frame,
                            parent: 0,
                            flags: 0,
                        },
                    )?;
                    let frame = m.read_word_with_backend(b, record)?;
                    b.game_init_effect(
                        m.reborrow(),
                        CarGameInitEffect::SetFrameOn {
                            frame,
                            enabled: false,
                        },
                    )?;
                }
                let target = m.read_word_with_backend(b, context + 0x68)?;
                let app = m.read_word_with_backend(b, 0x63788c)?;
                let game = m.read_word_with_backend(b, app + 0x24)?;
                // PC 5ae430: detach only entries whose parent chain contains
                // the supplied model, in the original forward array order.
                for i in 0..100 {
                    let frame = m.read_word_with_backend(b, game + 0x5b8 + i * 4)?;
                    if frame == 0 {
                        continue;
                    }
                    let mut parent = m.read_word_with_backend(b, frame + 0x120)?;
                    let mut remaining = 1 << 20;
                    while parent != 0 {
                        if parent == target {
                            b.game_init_effect(
                                m.reborrow(),
                                CarGameInitEffect::LinkFrame {
                                    frame,
                                    parent: 0,
                                    flags: 0,
                                },
                            )?;
                            break;
                        }
                        if remaining == 0 {
                            return Err(Error::UnavailableFrameMethod(
                                "Cyclic shadow receiver ancestry",
                            ));
                        }
                        remaining -= 1;
                        parent = m.read_word_with_backend(b, parent + 0x120)?;
                    }
                }
                Ok(0)
            }
            CarGameInitEffect::DynamicCollision {
                entry,
                manager,
                object,
            } => {
                let mut live = LiveServices {
                    backend: b,
                    memory: m,
                };
                let result = match entry {
                    0x5c36c0 => {
                        crate::dynamic_collision_manager::dyn_insert(&mut live, manager, object)
                    }
                    0x5c3ac0 => {
                        crate::dynamic_collision_manager::dyn_update(&mut live, manager, object)
                    }
                    0x5c38b0 => {
                        crate::dynamic_collision_manager::dyn_erase(&mut live, manager, object)
                    }
                    _ => return Err(Error::UnavailableGameDependency(entry)),
                };
                result
                    .map(|_| 0)
                    .map_err(|e| Error::UnavailableGameEffect(format!("Dynamic collision: {e:?}")))
            }
            _ => Err(Error::UnavailableGameEffect(format!("{e:?}"))),
        }
    }
    fn support_effect(
        &mut self,
        b: &mut Backend<'_>,
        m: VehicleComponentTailMemory<'_>,
        e: CarGameInitSupportEffect,
    ) -> Result<u32, Error> {
        match e {
            // The standalone scene has no registered shadow type library.
            // These are the original empty-container branches, with explicit
            // errors for contexts requiring further shadow renderer services.
            CarGameInitSupportEffect::FindShadowType { manager, .. }
                if m.read_word_with_backend(b, manager)? <= 1 =>
            {
                Ok(0)
            }
            CarGameInitSupportEffect::InstallShadow { game, kind, .. } => {
                let begin = m.read_word_with_backend(b, game + 0x284)?;
                if begin == 0 {
                    return Ok(0);
                }
                let end = m.read_word_with_backend(b, game + 0x288)?;
                if end.wrapping_sub(begin) / 8 <= kind {
                    return Ok(0);
                }
                Err(Error::UnavailableGameEffect(format!("{e:?}")))
            }
            CarGameInitSupportEffect::RemoveShadow { game, index, .. } => {
                let header = game + if index == 0 { 0x290 } else { 0x2a0 };
                let begin = m.read_word_with_backend(b, header + 4)?;
                if begin == 0 || m.read_word_with_backend(b, header + 8)? == begin {
                    return Ok(0);
                }
                Err(Error::UnavailableGameEffect(format!("{e:?}")))
            }
            _ => Err(Error::UnavailableGameEffect(format!("{e:?}"))),
        }
    }
    fn done_effect(
        &mut self,
        b: &mut Backend<'_>,
        mut m: VehicleComponentTailMemory<'_>,
        e: CarGameDoneEffect,
    ) -> Result<(), Error> {
        match e {
            CarGameDoneEffect::RemoveUsingObject { manager, object } => {
                if object == 0 {
                    return Ok(());
                }
                let begin = m.read_word_with_backend(b, manager + 4)?;
                if begin == 0 {
                    return Ok(());
                }
                let end = m.read_word_with_backend(b, manager + 8)?;
                let mut p = end;
                while p != begin {
                    p = p.wrapping_sub(4);
                    if m.read_word_with_backend(b, p)? != object {
                        continue;
                    }
                    let index = p.wrapping_sub(begin) / 4;
                    let model = m.read_word_with_backend(b, object + 0x18)?;
                    let flags = m.read_byte_with_backend(b, object + 0x14)?;
                    if model != 0 && flags & 2 != 0 {
                        return Err(Error::UnavailableGameEffect(
                            "RemoveUsingObject visual destruction".into(),
                        ));
                    }
                    if flags & 1 != 0 {
                        b.free_projected(&mut m, object)?;
                    }
                    let begin = m.read_word_with_backend(b, manager + 4)?;
                    let end = m.read_word_with_backend(b, manager + 8)?;
                    let value = m.read_word_with_backend(b, end - 4)?;
                    m.write_word_with_backend(b, begin + index * 4, value)?;
                    m.write_word_with_backend(b, manager + 8, end - 4)?;
                    return Ok(());
                }
                Ok(())
            }
            // Original C_radar removal (PC RVA 0x1fa720), including the
            // first-match erase and retained order of the eight-byte records.
            CarGameDoneEffect::RadarRemoveCar { manager, car } => {
                let begin = m.read_word_with_backend(b, manager + 0x449c)?;
                let end = m.read_word_with_backend(b, manager + 0x44a0)?;
                let mut p = begin;
                while p < end {
                    if m.read_word_with_backend(b, p)? == car {
                        let mut q = p + 8;
                        while q != end {
                            for offset in [0, 4] {
                                let v = m.read_word_with_backend(b, q + offset)?;
                                m.write_word_with_backend(b, p + offset, v)?;
                            }
                            p += 8;
                            q += 8;
                        }
                        m.write_word_with_backend(b, manager + 0x44a0, end - 8)?;
                        return Ok(());
                    }
                    p = p.wrapping_add(8);
                }
                Ok(())
            }
            _ => Err(Error::UnavailableGameEffect(format!("{e:?}"))),
        }
    }
}

/// A service may inspect or mutate embedded car fields through this same
/// borrowed projection, including allocator callbacks and partial failures.
struct LiveServices<'a, 'b, 'm> {
    backend: &'a mut Backend<'b>,
    memory: VehicleComponentTailMemory<'m>,
}
impl VehicleInitBackend for LiveServices<'_, '_, '_> {
    type Error = Error;
    fn read_byte(&mut self, a: u32) -> Result<u8, Error> {
        self.memory.read_byte_with_backend(self.backend, a)
    }
    fn read_word(&mut self, a: u32) -> Result<u32, Error> {
        self.memory.read_word_with_backend(self.backend, a)
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Error> {
        self.memory.write_byte_with_backend(self.backend, a, v)
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), Error> {
        self.memory.write_word_with_backend(self.backend, a, v)
    }
    fn allocate(&mut self, n: u32) -> Result<u32, Error> {
        self.backend.allocate_projected(&mut self.memory, n)
    }
    fn free(&mut self, a: u32) -> Result<(), Error> {
        self.backend.free_projected(&mut self.memory, a)
    }
    fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, f: u32) -> Result<(), Error> {
        self.backend.collect_frame(v, f)
    }
    fn enumerate(
        &mut self,
        f: u32,
        q: VehicleInitQuery,
        e: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Error>> {
        self.backend.enumerate(f, q, e)
    }
}
