//! Composition of the original six-float C_Vehicle::Move, PC RVA e2090.
//! All phases address one live vehicle and wheel/frame/resource backend.
//! Required body, wheel, drivetrain and frame effects are supplied explicitly;
//! this module does not construct a world or substitute missing collisions.
use crate::{
    vehicle_dynamics_body::{vehicle_move_integrate_body, VehicleRigidBodyBackend},
    vehicle_dynamics_forces::vehicle_move_force_setup,
    vehicle_dynamics_prepare::{vehicle_move_prepare, VehicleMovePrepareUnavailable},
    vehicle_dynamics_tail::{
        vehicle_move_tail, VehicleMoveTailBackend, VehicleMoveTailUnavailable,
    },
    vehicle_dynamics_wheel_forces::{vehicle_move_wheel_forces, VehicleMoveWheelForcesUnavailable},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleMoveUnavailable<E> {
    Prepare(VehicleMovePrepareUnavailable<E>),
    ForceSetup(VehicleMovePrepareUnavailable<E>),
    WheelForces(VehicleMoveWheelForcesUnavailable<E>),
    IntegrateBody(VehicleMovePrepareUnavailable<E>),
    Tail(VehicleMoveTailUnavailable<E>),
}

/// Arguments retain the original order: timestep, three aerodynamic drag
/// terms and two motor force channels. Errors retain every completed phase.
pub fn vehicle_move<B>(
    backend: &mut B,
    vehicle: u32,
    arguments: [f32; 6],
) -> Result<bool, VehicleMoveUnavailable<B::Error>>
where
    B: VehicleRigidBodyBackend + VehicleMoveTailBackend,
{
    let prepared = vehicle_move_prepare(backend, vehicle, arguments[0])
        .map_err(VehicleMoveUnavailable::Prepare)?;
    let setup = vehicle_move_force_setup(backend, vehicle, arguments, prepared)
        .map_err(VehicleMoveUnavailable::ForceSetup)?;
    let forces = vehicle_move_wheel_forces(backend, vehicle, prepared, setup)
        .map_err(VehicleMoveUnavailable::WheelForces)?;
    vehicle_move_integrate_body(backend, vehicle, arguments[0], setup, forces.into())
        .map_err(VehicleMoveUnavailable::IntegrateBody)?;
    vehicle_move_tail(backend, vehicle, arguments).map_err(VehicleMoveUnavailable::Tail)
}
