//! Wheel pose effects use the existing ModelArena and initialized heap owner.
use super::*;
use crate::vehicle_wheel_pose::VehicleWheelPoseBackend;

impl<B: VehicleComponentsBackend> VehicleWheelPoseBackend for VehicleFrameBackend<'_, B> {
    type Error = VehicleFrameError<B::Error>;
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        VehicleInitBackend::read_word(self, address)
    }
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        VehicleInitBackend::read_byte(self, address)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        VehicleInitBackend::write_word(self, address, value)
    }
    fn make_axis_angle(&mut self, axis: [f32; 3], angle: f32) -> Result<SQuat, Self::Error> {
        SQuat::try_make_axis_angle_reduced(axis, angle)
            .map_err(|_| VehicleFrameError::NumericDomain("wheel quaternion Make"))
    }
    fn normalize_frame_rotation(&mut self, frame: u32) -> Result<(), Self::Error> {
        VehicleComponentsBackend::normalize_frame_quaternion(self, frame)
    }
    fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        VehicleComponentsBackend::set_frame_rotation_matrix(self, frame)
    }
}
