//! Composition of the original Init entry, all reached helpers, and native
//! temporary cleanup. A failed call retains persistent constructor ownership.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleCompleteInitRejection {
    Component(VehicleInitRejection),
    Wheel(VehicleWheelRejection),
    MissingCentre,
    Setup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleCompleteInitOutcome {
    Rejected(VehicleCompleteInitRejection),
    /// Original C_Vehicle::Init returned AL=1 after temporary-vector cleanup.
    Initialized,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleCompleteInitUnavailable<E> {
    Component(VehicleComponentTailUnavailable<E>),
    Remainder(VehicleWheelsUnavailable<E>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleCarInitOutcome {
    BaseActorRejected,
    VehicleRejected(VehicleCompleteInitRejection),
    /// Native C_car::Init VA 0x41bdf2, after Vehicle::Init succeeded and before
    /// callback installation, actor/audio/physics registration, or car success.
    PostVehicleFrontier,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleCarInitUnavailable<E> {
    Caller(VehicleCarCallerUnavailable<E>),
    Vehicle(VehicleCompleteInitUnavailable<E>),
}

impl VehicleInitState {
    /// Complete original C_Vehicle::Init over an already constructed vehicle.
    /// The backend maps helper stack storage below `storage.scratch.address`,
    /// including Setup's 0x1218-byte frame and its nested calls.
    /// Host unavailability consumes the reached continuation and preserves all
    /// completed effects. Rejection never implicitly invokes the destructor.
    pub fn init<B>(
        &mut self,
        backend: &mut B,
        model: u32,
        storage: VehicleComponentTailStorage,
    ) -> Result<VehicleCompleteInitOutcome, VehicleCompleteInitUnavailable<B::Error>>
    where
        B: VehicleComponentTailBackend
            + VehicleWheelsBackend
            + VehicleAxisBackend
            + VehicleAxisUpdateBackend
            + VehicleInitFinishBackend
            + VehicleSetupBackend,
    {
        use VehicleCompleteInitOutcome::Rejected;
        use VehicleCompleteInitRejection::{Component, MissingCentre, Wheel};
        use VehicleCompleteInitUnavailable::{Component as ComponentError, Remainder};
        match self
            .init_component_tail(backend, model, storage)
            .map_err(ComponentError)?
        {
            VehicleComponentTailOutcome::Rejected(reason) => {
                return Ok(Rejected(Component(reason)));
            }
            VehicleComponentTailOutcome::UnavailableFrontier(
                VehicleComponentTailFrontier::WheelEnumeration,
            ) => {}
        }
        match self.continue_wheels(backend).map_err(Remainder)? {
            VehicleWheelsOutcome::Rejected(reason) => return Ok(Rejected(Wheel(reason))),
            VehicleWheelsOutcome::UnavailableFrontier(VehicleWheelsFrontier::ParameterSetup) => {}
        }
        match self.continue_init_geometry(backend).map_err(Remainder)? {
            VehicleInitGeometryOutcome::MissingCentre => return Ok(Rejected(MissingCentre)),
            VehicleInitGeometryOutcome::FinalSetup => {}
        }
        self.continue_init_final(backend).map_err(Remainder)
    }

    /// Consumes only the genuine final-setup continuation. Both native boolean
    /// returns release Init's four temporary vectors in their original order.
    pub fn continue_init_final<B: VehicleSetupBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<VehicleCompleteInitOutcome, VehicleWheelsUnavailable<B::Error>> {
        if !std::mem::take(&mut self.final_setup_ready) {
            return Err(VehicleWheelsUnavailable::NoContinuation);
        }
        let parent_stack = self.tail_scratch.as_ref().unwrap().address;
        let stack = parent_stack.wrapping_sub(0x1218);
        let vehicle = self.tail_vehicle_address.unwrap();
        let saved_esi = self
            .init_geometry_saved_esi
            .ok_or(VehicleWheelsUnavailable::NoContinuation)?;
        // Actual CALL, SEH registration, stack-probe call, and PUSH register
        // order. Stack-probe page reads do not initialize automatic bytes.
        for (offset, value) in [
            (0x1214, 0x4bf988),
            (0x1210, u32::MAX),
            (0x120c, 0x61eafb),
            (0x1208, parent_stack.wrapping_add(0x1bc)),
            (0x1204, 0x4bfa6f),
            (0x1200, vehicle),
            (0xc, 0x4bfa6f),
            (0xc, 0),
            (0x8, vehicle),
            (0x4, saved_esi),
            (0, vehicle.wrapping_add(0x17c)),
        ] {
            self.wheel_store(backend, stack.wrapping_add(offset), value)?;
        }
        self.final_setup_ready = true;
        let success = self.setup(backend, stack)?;
        self.release_init_vectors(backend)?;
        Ok(if success {
            VehicleCompleteInitOutcome::Initialized
        } else {
            VehicleCompleteInitOutcome::Rejected(VehicleCompleteInitRejection::Setup)
        })
    }

    /// Actual C_car entry and embedded Vehicle::Init call. Actor storage before
    /// car+0x70 and the shared actor-id counter belong to the same backend.
    /// This API deliberately retains the original later C_car call frontier.
    pub fn init_from_car<B>(
        &mut self,
        backend: &mut B,
        car: u32,
        model: u32,
        storage: VehicleComponentTailStorage,
    ) -> Result<VehicleCarInitOutcome, VehicleCarInitUnavailable<B::Error>>
    where
        B: VehicleCarCallerBackend
            + VehicleComponentTailBackend
            + VehicleWheelsBackend
            + VehicleAxisBackend
            + VehicleAxisUpdateBackend
            + VehicleInitFinishBackend
            + VehicleSetupBackend,
    {
        if !self
            .prepare_car_init(backend, car, model, &storage)
            .map_err(VehicleCarInitUnavailable::Caller)?
        {
            return Ok(VehicleCarInitOutcome::BaseActorRejected);
        }
        match self
            .init(backend, model, storage)
            .map_err(VehicleCarInitUnavailable::Vehicle)?
        {
            VehicleCompleteInitOutcome::Rejected(reason) => {
                Ok(VehicleCarInitOutcome::VehicleRejected(reason))
            }
            VehicleCompleteInitOutcome::Initialized => {
                Ok(VehicleCarInitOutcome::PostVehicleFrontier)
            }
        }
    }
}
