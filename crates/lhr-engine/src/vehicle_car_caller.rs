//! Original C_car::Init entry through its embedded C_Vehicle::Init call.
//! The latter subobject starts at car+0x70. A false native Init result returns
//! directly; the caller neither destroys the vehicle nor releases its model.
use super::component_tail::Projection;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleActorTarget {
    End,
    Next,
}
impl VehicleActorTarget {
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Self::End => b"targetE",
            Self::Next => b"targetN",
        }
    }
    pub fn mask(self) -> u32 {
        0x40
    }
}
pub trait VehicleCarCallerBackend: VehicleInitBackend {
    fn find_actor_target(
        &mut self,
        model: u32,
        target: VehicleActorTarget,
    ) -> Result<u32, Self::Error>;
    fn set_actor_target_on(&mut self, frame: u32, on: bool) -> Result<(), Self::Error>;
}
impl<B: VehicleCarCallerBackend> VehicleCarCallerBackend for Projection<'_, B> {
    fn find_actor_target(
        &mut self,
        model: u32,
        target: VehicleActorTarget,
    ) -> Result<u32, Self::Error> {
        self.backend.find_actor_target(model, target)
    }
    fn set_actor_target_on(&mut self, frame: u32, on: bool) -> Result<(), Self::Error> {
        self.backend.set_actor_target_on(frame, on)
    }
}
/// Original actor attachment at PC RVA0x5dd0. All addresses are resolved through
/// the backend, including its shared actor-id counter and existing owner pointer.
/// Successful attachment does not acquire a model reference in this function.
pub fn vehicle_actor_init<B: VehicleCarCallerBackend>(
    b: &mut B,
    actor: u32,
    model: u32,
) -> Result<bool, B::Error> {
    if model == 0 {
        return Ok(false);
    }
    let pointer = b.read_word(model.wrapping_add(8))?;
    if pointer != 0 {
        let owner = b.read_word(pointer)?;
        if owner != 0 && owner != actor {
            return Ok(false);
        }
    }
    b.write_word(actor.wrapping_add(0x68), model)?;
    b.write_word(model.wrapping_add(8), 0)?;
    let context = b.read_word(0x63788c)?;
    let id = b.read_word(context.wrapping_add(8))?.wrapping_add(1);
    b.write_word(context.wrapping_add(8), id)?;
    b.write_word(actor.wrapping_add(0x18), id)?;
    let model = b.read_word(actor.wrapping_add(0x68))?;
    b.write_byte(actor.wrapping_add(0x6c), 0)?;
    if model != 0 {
        let target = b.find_actor_target(model, VehicleActorTarget::End)?;
        if target != 0 {
            b.set_actor_target_on(target, false)?;
        }
        let model = b.read_word(actor.wrapping_add(0x68))?;
        let target = b.find_actor_target(model, VehicleActorTarget::Next)?;
        if target != 0 {
            b.set_actor_target_on(target, false)?;
        }
    }
    b.write_word(actor.wrapping_add(0x1c), 0)?;
    Ok(true)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleCarCallerUnavailable<E> {
    Init(VehicleInitUnavailable<E>),
    Storage,
    AlreadyStarted,
}
impl VehicleInitState {
    /// Original C_car entry prefix up to the genuine Vehicle::Init callsite.
    /// The actor's first0x70 bytes are backend-owned; this projection owns the
    /// vehicle subobject at car+0x70. Call only as part of a single Init attempt.
    /// Returns false exactly when the original base actor rejects attachment.
    pub fn prepare_car_init<B: VehicleCarCallerBackend>(
        &mut self,
        b: &mut B,
        car: u32,
        model: u32,
        storage: &VehicleComponentTailStorage,
    ) -> Result<bool, VehicleCarCallerUnavailable<B::Error>> {
        if self.started || self.car_caller_started {
            return Err(VehicleCarCallerUnavailable::AlreadyStarted);
        }
        self.car_caller_started = true;
        if car.checked_add(0x70) != Some(storage.bodycol.vehicle_address) {
            return Err(VehicleCarCallerUnavailable::Storage);
        }
        self.validate_tail_storage::<B>(storage)
            .map_err(|_| VehicleCarCallerUnavailable::Storage)?;
        self.tail_vehicle_address = Some(storage.bodycol.vehicle_address);
        self.tail_scratch = Some(storage.scratch.clone());
        let mut memory = self.tail_projection(b, storage.bodycol.vehicle_address);
        memory
            .write_word(car.wrapping_add(0x21d8), 0)
            .map_err(|e| VehicleCarCallerUnavailable::Init(VehicleInitUnavailable::Effect(e)))?;
        if !vehicle_actor_init(&mut memory, car, model)
            .map_err(|e| VehicleCarCallerUnavailable::Init(VehicleInitUnavailable::Effect(e)))?
        {
            return Ok(false);
        }
        for (offset, value) in [
            (0x20bc, 0x42c80000),
            (0x20b4, 0x41dd999a),
            (0x20b8, 0x40400000),
            (0x20c0, 0x41200000),
            (0x20c4, 0x42c80000),
        ] {
            memory
                .write_word(car.wrapping_add(offset), value)
                .map_err(|e| {
                    VehicleCarCallerUnavailable::Init(VehicleInitUnavailable::Effect(e))
                })?;
        }
        memory
            .write_byte(car.wrapping_add(0x20c8), 0)
            .map_err(|e| VehicleCarCallerUnavailable::Init(VehicleInitUnavailable::Effect(e)))?;
        self.car_caller_ready = true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::super::wheels::tests::{Effect, Memory};
    use super::*;
    impl VehicleCarCallerBackend for Memory {
        fn find_actor_target(&mut self, model: u32, target: VehicleActorTarget) -> Result<u32, ()> {
            self.effect(Effect::Find(model, target))?;
            Ok(match target {
                VehicleActorTarget::End => 0x3000,
                VehicleActorTarget::Next => 0x3200,
            })
        }
        fn set_actor_target_on(&mut self, frame: u32, on: bool) -> Result<(), ()> {
            self.effect(Effect::On(frame, on))
        }
    }
    fn fixture() -> Memory {
        let mut b = Memory::default();
        b.put(0x10000, &[0xa5; 0x2400]);
        b.word(0x2008, 0);
        b.word(0x63788c, 0x4000);
        b.word(0x4008, u32::MAX);
        b
    }
    #[test]
    fn actor_attachment_uses_shared_wrapping_counter_without_model_addref() {
        let mut b = fixture();
        assert_eq!(vehicle_actor_init(&mut b, 0x10000, 0x2000), Ok(true));
        assert_eq!(b.get(0x10068), 0x2000);
        assert_eq!(b.get(0x10018), 0);
        assert_eq!(b.get(0x4008), 0);
        assert_eq!(b.get(0x1001c), 0);
        assert_eq!(b.bytes[&0x1006c], 0);
        assert_eq!(b.get(0x2004), 0xa5a5a5a5);
        assert!(b.effects.ends_with(&[
            Effect::On(0x3200, false),
            Effect::Write(0x1001c, vec![0; 4])
        ]));
    }
    #[test]
    fn other_actor_owner_rejects_without_releases_or_attachment_stores() {
        let mut b = fixture();
        b.word(0x2008, 0x5000);
        b.word(0x5000, 0x6000);
        assert_eq!(vehicle_actor_init(&mut b, 0x10000, 0x2000), Ok(false));
        assert_eq!(b.get(0x10068), 0xa5a5a5a5);
        assert_eq!(
            b.effects,
            [Effect::Read(0x2008, 4), Effect::Read(0x5000, 4)]
        );
    }
    #[test]
    fn actor_attachment_unavailability_is_not_native_rejection_or_cleanup() {
        let initial = fixture();
        let mut complete = initial.clone();
        vehicle_actor_init(&mut complete, 0x10000, 0x2000).unwrap();
        for fail in 0..complete.effects.len() {
            let mut b = initial.clone();
            b.fail = Some(fail);
            assert_eq!(vehicle_actor_init(&mut b, 0x10000, 0x2000), Err(()));
            assert_eq!(b.effects, complete.effects[..fail]);
            assert!(!b
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Free(_) | Effect::Release(_))));
        }
    }
}
