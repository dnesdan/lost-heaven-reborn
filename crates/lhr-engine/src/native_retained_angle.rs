//! Host value for the unspilled result of the original S_vector::AngleTo.
//! It owns no frame or resource and loads no original binary at runtime.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
use super::native_trig::vector_angle::{
    try_portable_retained_vector_angle as backend_angle,
    PortableRetainedVectorAngle as BackendAngle,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::vector_x87::{
    try_retained_vector_angle as backend_angle, RetainedVectorAngle as BackendAngle,
};
use super::MakeUnavailable;

pub struct RetainedVectorAngle {
    value: BackendAngle,
}
pub fn try_retained_vector_angle(
    first: [f32; 3],
    second: [f32; 3],
) -> Result<RetainedVectorAngle, MakeUnavailable> {
    Ok(RetainedVectorAngle {
        value: backend_angle(first, second)?,
    })
}
impl RetainedVectorAngle {
    /// Original FCOM/FCOMP against a binary32 memory operand, without spilling
    /// the retained 80-bit return. Portable overlap stays unavailable.
    pub fn compare(&self, operand: f32) -> Result<std::cmp::Ordering, MakeUnavailable> {
        self.value.compare(operand)
    }
    /// Original FST binary32, retaining the angle for subsequent arithmetic.
    pub fn spill(&self) -> Result<f32, MakeUnavailable> {
        self.value.spill()
    }
    /// Original FLD numerator; FDIV ST0,ST1; FSTP binary32. The caller reads
    /// numerator only after publishing the earlier FST, as Engine requires.
    pub fn divide_numerator(self, numerator: f32) -> Result<f32, MakeUnavailable> {
        self.value.divide_numerator(numerator)
    }
}
