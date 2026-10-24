//! Native x87 implementation of retained Move/contact expressions.
//!
//! The CPU supplies the same retained transcendental values as original x86
//! execution. No original binary is loaded. Each assembly block saves and
//! restores the complete incoming x87 environment and eight-register stack,
//! including CW/status/tags and full instruction/data pointers. On x86_64 the
//! same block also restores the captured MXCSR/XMM state with FXRSTOR64. No
//! instruction changes upper AVX registers; ordinary Rust guards follow the
//! platform calling convention. Ordinary finite inputs and outputs remain guarded. CRT acos uses the original default handler mode,
//! including its precision-status-dependent qword spill; alternate CRT handler
//! configuration is outside this numeric profile. This is not general libm.
use crate::native_transform::MakeUnavailable;
use std::arch::asm;

fn number(x: f32) -> Result<f32, MakeUnavailable> {
    if x == 0. || x.is_normal() {
        Ok(x)
    } else {
        Err(MakeUnavailable::InputOutsideVerifiedDomain)
    }
}

#[derive(Clone, Copy)]
struct Input {
    first: [f32; 3],
    second: [f32; 3],
    cosine: f32,
    mode: u32,
}
impl Input {
    fn vectors(first: [f32; 3], second: [f32; 3]) -> Result<Self, MakeUnavailable> {
        for x in first.into_iter().chain(second) {
            number(x)?;
        }
        Ok(Self {
            first,
            second,
            cosine: 0.,
            mode: 1,
        })
    }
    fn cosine(value: f32) -> Self {
        Self {
            first: [0.; 3],
            second: [0.; 3],
            cosine: value,
            mode: 0,
        }
    }
}

// The long-mode legacy 108-byte image truncates FIP/FDP. FXSAVE64 preserves
// those pointers and SSE state; FNINIT is nonwaiting even for a saved unmasked
// pending exception. On 32-bit x86 the legacy image contains complete pointers.
#[cfg(target_arch = "x86_64")]
const SAVED_BYTES: usize = 512;
#[cfg(target_arch = "x86")]
const SAVED_BYTES: usize = 108;
#[cfg(target_arch = "x86_64")]
const SAVED_STATUS_OFFSET: usize = 2;
#[cfg(target_arch = "x86")]
const SAVED_STATUS_OFFSET: usize = 4;
#[repr(C, align(16))]
struct SavedEnvironment([u8; SAVED_BYTES]);

#[cfg(target_arch = "x86_64")]
macro_rules! save_environment {
    () => {
        "fxsave64 [{saved}]\nfninit"
    };
}
#[cfg(target_arch = "x86")]
macro_rules! save_environment {
    () => {
        "fnsave [{saved}]"
    };
}
#[cfg(target_arch = "x86_64")]
macro_rules! restore_environment {
    () => {
        "fxrstor64 [{saved}]"
    };
}
#[cfg(target_arch = "x86")]
macro_rules! restore_environment {
    () => {
        "frstor [{saved}]"
    };
}

// All arithmetic, including the AngleTo preprocessing which sets sticky
// precision, executes after the save and before the final restore. No f64
// reconstruction of x87 exception status is used.
#[repr(C, align(16))]
struct Context {
    control: u16,
    reserved: u16,
    scalar: f32,
    mode: u32,
    cosine: f32,
    first_length: f32,
    reserved_length: u32,
    narrowed: f64,
    first: [f32; 3],
    second: [f32; 3],
    squared: f32,
    cutoff: f32,
    epsilon: f64,
    status: u16,
    reserved_status: u16,
    negative_one: f32,
    incoming_precision: u32,
}
impl Context {
    fn new(input: Input) -> Self {
        Self {
            control: 0x007f,
            reserved: 0,
            scalar: 1.,
            mode: input.mode,
            cosine: input.cosine,
            first_length: 0.,
            reserved_length: 0,
            narrowed: 0.,
            first: input.first,
            second: input.second,
            squared: 0.,
            cutoff: f32::from_bits(0x322bcc77),
            epsilon: f64::from(f32::from_bits(0x322bcc77)),
            status: 0,
            reserved_status: 0,
            negative_one: -1.,
            incoming_precision: crate::native_precision::current_precision()
                .map(|raised| 0x10000 | if raised { 0x20 } else { 0 })
                .unwrap_or(0),
        }
    }
    fn record_precision(&self) {
        crate::native_precision::record_precision(self.status & 0x20 != 0);
    }
    fn finish(&self) -> Result<(), MakeUnavailable> {
        self.record_precision();
        // Invalid, denormal, zero-divide and exponent-range exceptions concern
        // the isolated calculation only, never a caller's restored status.
        if self.status & 0x1f != 0 {
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        } else {
            Ok(())
        }
    }
}

macro_rules! expression {
    ($input:expr,$out:expr,$params:expr,$($instruction:literal),+ $(,)?)=>{{
        let mut state=Context::new($input);let mut saved=SavedEnvironment([0;SAVED_BYTES]);
        let params=$params;let out=$out;
        // SAFETY: operands are disjoint live local storage. Every branch joins
        // before the final restore, and no Rust call/unwind occurs in the block.
        unsafe {asm!(
            save_environment!(), "fldcw word ptr [{state}+0]",
            "cmp dword ptr [{state}+8], 0", "je 7f",
            // First norm: X*X + Y*Y + Z*Z, with the original binary32 spill.
            "fld dword ptr [{state}+32]", "fmul dword ptr [{state}+32]",
            "fld dword ptr [{state}+36]", "fmul dword ptr [{state}+36]", "faddp st(1), st(0)",
            "fld dword ptr [{state}+40]", "fmul dword ptr [{state}+40]", "faddp st(1), st(0)",
            "fstp dword ptr [{state}+56]",
            "mov eax, dword ptr [{state}+56]", "cmp eax, 0x322bcc77", "jl 62f",
            "fld dword ptr [{state}+56]", "fsub dword ptr [{state}+4]", "fabs",
            "fcomp qword ptr [{state}+64]", "fnstsw ax", "test ah, 5", "jnp 63f",
            "fld dword ptr [{state}+56]", "fsqrt", "fstp dword ptr [{state}+16]", "jmp 64f",
            "62:", "mov dword ptr [{state}+16], 0", "jmp 64f",
            "63:", "mov dword ptr [{state}+16], 0x3f800000",
            "64:",
            // Second norm has a retained square root until the product.
            "fld dword ptr [{state}+44]", "fmul dword ptr [{state}+44]",
            "fld dword ptr [{state}+48]", "fmul dword ptr [{state}+48]", "faddp st(1), st(0)",
            "fld dword ptr [{state}+52]", "fmul dword ptr [{state}+52]", "faddp st(1), st(0)",
            "fstp dword ptr [{state}+56]",
            "mov eax, dword ptr [{state}+56]", "cmp eax, 0x322bcc77", "jl 72f",
            "fld dword ptr [{state}+56]", "fsub dword ptr [{state}+4]", "fabs",
            "fcomp qword ptr [{state}+64]", "fnstsw ax", "test ah, 5", "jnp 73f",
            "fld dword ptr [{state}+56]", "fsqrt", "jmp 74f",
            "72:", "fldz", "jmp 74f", "73:", "fld1", "74:",
            "fmul dword ptr [{state}+16]",
            "fcom dword ptr [{state}+60]", "fnstsw ax", "test ah, 5", "jp 75f",
            // A degenerate product returns +0 directly, without CRT acos.
            "fstp st(0)", "fldz", "jmp 5f",
            "75:",
            // Dot accumulates X,Z,Y before division and a binary32 spill.
            "fld dword ptr [{state}+32]", "fmul dword ptr [{state}+44]",
            "fld dword ptr [{state}+40]", "fmul dword ptr [{state}+52]", "faddp st(1), st(0)",
            "fld dword ptr [{state}+36]", "fmul dword ptr [{state}+48]", "faddp st(1), st(0)",
            "fdiv st(0), st(1)", "fstp dword ptr [{state}+12]", "fstp st(0)",
            // Retain the original comparisons and +/-1 clamping.
            "fld dword ptr [{state}+12]", "fcomp dword ptr [{state}+76]",
            "fnstsw ax", "test ah, 0x41", "jnz 76f",
            "fld dword ptr [{state}+12]", "fcomp dword ptr [{state}+4]",
            "fnstsw ax", "test ah, 5", "jp 77f", "jmp 7f",
            "76:", "mov dword ptr [{state}+12], 0xbf800000", "jmp 7f",
            "77:", "mov dword ptr [{state}+12], 0x3f800000",
            "7:",
            "mov eax, dword ptr [{state}+12]", "cmp eax, 0x3f800000", "je 2f",
            "cmp eax, 0xbf800000", "je 3f",
            "fld dword ptr [{state}+12]", "fld1", "fadd st(0), st(1)",
            "fld1", "fsub st(0), st(2)", "fmulp st(1), st(0)", "fsqrt", "fxch st(1)", "fpatan", "jmp 4f",
            "2:", "fldz", "jmp 4f", "3:", "fldpi", "4:",
            // A scoped original pipeline supplies incoming PE explicitly.
            // Without one, use the actual saved caller PE as before. The
            // isolated calculation's real PE participates in either mode.
            "fnstsw ax", "test dword ptr [{state}+80], 0x10000", "jz 78f",
            "or ax, word ptr [{state}+80]", "jmp 79f",
            "78:", "or ax, word ptr [{saved}+{saved_status}]", "79:",
            "test ax, 0x20", "jz 5f",
            "fstp qword ptr [{state}+24]", "fld qword ptr [{state}+24]", "5:",
            $($instruction,)+
            "fnstsw word ptr [{state}+72]", restore_environment!(),
            state=in(reg)&mut state,saved=in(reg)&mut saved,saved_status=const SAVED_STATUS_OFFSET,params=in(reg)params.as_ptr(),out=in(reg)out.as_mut_ptr(),out("eax")_,options(nostack),
        );}
        state.finish()?;
    }};
}

/// Opaque original AngleTo ST0 value before any caller binary32 spill.
/// Its ten bytes are produced only by the isolated checked x87 calculation.
#[derive(Clone, Copy)]
pub(crate) struct RetainedVectorAngle {
    value: [u8; 10],
}

pub(crate) fn try_retained_vector_angle(
    first: [f32; 3],
    second: [f32; 3],
) -> Result<RetainedVectorAngle, MakeUnavailable> {
    let input = Input::vectors(first, second)?;
    let mut value = [0_u8; 10];
    expression!(
        input,
        &mut value,
        [0_f32; 1],
        "fstp tbyte ptr [{out}]",
        "mov eax, dword ptr [{params}]",
    );
    Ok(RetainedVectorAngle { value })
}

impl RetainedVectorAngle {
    /// FCOMP m32real preserves the unspilled AngleTo return through the
    /// caller's branch. Only the comparison condition codes are observed.
    pub(crate) fn compare(&self, operand: f32) -> Result<std::cmp::Ordering, MakeUnavailable> {
        number(operand)?;
        let mut state = Context::new(Input::cosine(0.));
        state.scalar = operand;
        let mut saved = SavedEnvironment([0; SAVED_BYTES]);
        // SAFETY: all operands are live disjoint locals; the original complete
        // environment is restored before any Rust call or availability check.
        unsafe {
            asm!(
                save_environment!(), "fldcw word ptr [{state}+0]",
                "fld tbyte ptr [{value}]", "fcomp dword ptr [{state}+4]",
                "fnstsw word ptr [{state}+72]", restore_environment!(),
                state=in(reg)&mut state,saved=in(reg)&mut saved,value=in(reg)self.value.as_ptr(),
                options(nostack),
            );
        }
        state.finish()?;
        match state.status & 0x4500 {
            0 => Ok(std::cmp::Ordering::Greater),
            0x0100 => Ok(std::cmp::Ordering::Less),
            0x4000 => Ok(std::cmp::Ordering::Equal),
            _ => Err(MakeUnavailable::InputOutsideVerifiedDomain),
        }
    }
    /// The caller's FST binary32 result, without changing the retained token.
    pub(crate) fn spill(&self) -> Result<f32, MakeUnavailable> {
        self.evaluate(None)
    }

    /// Engine PC e1fa3..e1fb1: FLD numerator, FDIV ST0,ST1, FSTP binary32.
    /// Callers perform their first spill/store before reading this numerator.
    pub(crate) fn divide_numerator(&self, numerator: f32) -> Result<f32, MakeUnavailable> {
        number(numerator)?;
        self.evaluate(Some(numerator))
    }

    fn evaluate(&self, numerator: Option<f32>) -> Result<f32, MakeUnavailable> {
        let mut state = Context::new(Input::cosine(0.));
        state.scalar = numerator.unwrap_or(0.);
        state.mode = u32::from(numerator.is_some());
        let mut saved = SavedEnvironment([0; SAVED_BYTES]);
        let mut output = 0_f32;
        // SAFETY: the private token contains a validated x87 result. All
        // branches converge before the last full environment restore; no
        // call or unwind can occur while the incoming state is displaced.
        unsafe {
            asm!(
                save_environment!(), "fldcw word ptr [{state}+0]",
                "fld tbyte ptr [{value}]", "cmp dword ptr [{state}+8], 0", "je 2f",
                "fld dword ptr [{state}+4]", "fdiv st(0), st(1)",
                "fstp dword ptr [{output}]", "fstp st(0)", "jmp 3f",
                "2:", "fstp dword ptr [{output}]", "3:",
                "fnstsw word ptr [{state}+72]", restore_environment!(),
                state=in(reg)&mut state,saved=in(reg)&mut saved,value=in(reg)self.value.as_ptr(),
                output=in(reg)&mut output,options(nostack),
            );
        }
        state.finish()?;
        number(output)
    }
}

/// Original AngleTo result when the caller actually spills a binary32 return.
#[cfg(test)]
fn try_vector_angle_to(first: [f32; 3], second: [f32; 3]) -> Result<f32, MakeUnavailable> {
    try_retained_vector_angle(first, second)?.spill()
}

pub(crate) fn try_vector_angle_sine(
    first: [f32; 3],
    second: [f32; 3],
) -> Result<f32, MakeUnavailable> {
    let input = Input::vectors(first, second)?;
    let mut out = [0_f32; 1];
    expression!(
        input,
        &mut out,
        [0_f32; 1],
        "fsin",
        "fstp dword ptr [{out}]",
        // Keep the common parameter pointer as a documented operand.
        "mov eax, dword ptr [{params}]",
    );
    number(out[0])
}

/// AngleTo/FSIN retained once and multiplied independently by three scalars.
/// Body collision uses these products before its subsequent vector stores.
pub(crate) fn try_vector_angle_sine_scaled(
    first: [f32; 3],
    second: [f32; 3],
    factors: [f32; 3],
) -> Result<[f32; 3], MakeUnavailable> {
    let input = Input::vectors(first, second)?;
    for x in factors {
        number(x)?;
    }
    let mut out = [0_f32; 3];
    expression!(
        input,
        &mut out,
        factors,
        "fsin",
        "fld st(0)",
        "fmul dword ptr [{params}]",
        "fstp dword ptr [{out}]",
        "fld st(0)",
        "fmul dword ptr [{params}+4]",
        "fstp dword ptr [{out}+4]",
        "fld st(0)",
        "fmul dword ptr [{params}+8]",
        "fstp dword ptr [{out}+8]",
        "fstp st(0)",
    );
    for x in out {
        number(x)?;
    }
    Ok(out)
}

/// C_PhThing CollisionCount45e510's retained AngleTo/FSIN suffix. The
/// magnitude product and clamp occur before the three component FST stores.
pub(crate) fn try_ph_thing_friction_products(
    first: [f32; 3],
    second: [f32; 3],
    magnitude: f32,
    friction: f32,
    timestep: f32,
    mass: f32,
    axis: [f32; 3],
) -> Result<[f32; 3], MakeUnavailable> {
    let input = Input::vectors(first, second)?;
    for value in [magnitude, friction, timestep, mass]
        .into_iter()
        .chain(axis)
    {
        number(value)?;
    }
    let mut out = [0_f32; 4];
    expression!(
        input,
        &mut out,
        [magnitude, friction, timestep, mass, 3., axis[0], axis[1], axis[2]],
        "fsin",
        "fld dword ptr [{params}]",
        "fmul dword ptr [{params}+4]",
        "fmulp st(1), st(0)",
        "fld dword ptr [{params}+8]",
        "fmul dword ptr [{params}+4]",
        "fmul dword ptr [{params}+12]",
        "fmul dword ptr [{params}+16]",
        "fstp dword ptr [{out}+12]",
        "fcom dword ptr [{out}+12]",
        "fnstsw ax",
        "and eax, 0x4100",
        "jnz 81f",
        "fstp st(0)",
        "fld dword ptr [{out}+12]",
        "81:",
        "fld dword ptr [{params}+20]",
        "fmul st(0), st(1)",
        "fstp dword ptr [{out}]",
        "fld dword ptr [{params}+24]",
        "fmul st(0), st(1)",
        "fstp dword ptr [{out}+4]",
        "fld dword ptr [{params}+28]",
        "fmul st(0), st(1)",
        "fstp dword ptr [{out}+8]",
        "fstp st(0)",
    );
    for value in out {
        number(value)?;
    }
    Ok([out[0], out[1], out[2]])
}

/// Move PC e3d47..e3db7, preserving the division/multiply order and every spill.
pub(crate) fn try_wheel_friction_limit(
    first: [f32; 3],
    second: [f32; 3],
    max_long: f32,
    max_lat: f32,
) -> Result<[f32; 3], MakeUnavailable> {
    let input = Input::vectors(first, second)?;
    number(max_long)?;
    number(max_lat)?;
    let params = [f32::from_bits(0x3fc90fdb), max_long, max_lat];
    let mut out = [0_f32; 3];
    expression!(
        input,
        &mut out,
        params,
        "fsub dword ptr [{params}]",
        "fptan",
        "fstp st(0)",
        "fld dword ptr [{params} + 8]",
        "fld dword ptr [{params} + 4]",
        "fld st(1)",
        "fmulp st(2), st(0)",
        "fxch st(1)",
        "fmul st(0), st(2)",
        "fmul st(0), st(2)",
        "fld st(1)",
        "fmul st(0), st(2)",
        "faddp st(1), st(0)",
        "fstp st(1)",
        "fld st(0)",
        "fsqrt",
        "fld dword ptr [{params} + 4]",
        "fdiv st(0), st(2)",
        "fmulp st(1), st(0)",
        "fmul dword ptr [{params} + 8]",
        "fstp dword ptr [{out}]",
        "fstp st(0)",
        "fld dword ptr [{out}]",
        "fmul st(0), st(1)",
        "fstp dword ptr [{out} + 4]",
        "fstp st(0)",
        "fld dword ptr [{out} + 4]",
        "fmul dword ptr [{out} + 4]",
        "fld dword ptr [{out}]",
        "fmul dword ptr [{out}]",
        "faddp st(1), st(0)",
        "fsqrt",
        "fstp dword ptr [{out} + 8]",
    );
    for x in out {
        number(x)?;
    }
    Ok(out)
}

pub(crate) fn try_acos_sin_cos_products(
    argument: f32,
    sine_factor: f32,
    cosine_factor: f32,
) -> Result<[f32; 2], MakeUnavailable> {
    number(argument)?;
    number(sine_factor)?;
    number(cosine_factor)?;
    if argument.abs() > 1. {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let mut out = [0_f32; 2];
    expression!(
        Input::cosine(argument),
        &mut out,
        [sine_factor, cosine_factor],
        "fld st(0)",
        "fsin",
        "fmul dword ptr [{params}]",
        "fstp dword ptr [{out}]",
        "fcos",
        "fmul dword ptr [{params} + 4]",
        "fstp dword ptr [{out} + 4]",
    );
    for x in out {
        number(x)?;
    }
    Ok(out)
}

/// DoWheelsCollision's retained FCOS followed by its PC24 multiplication.
/// x87 argument reduction is available for magnitudes strictly below 2^63.
pub(crate) fn try_cosine_product(angle: f32, factor: f32) -> Result<f32, MakeUnavailable> {
    number(angle)?;
    number(factor)?;
    if angle.abs() >= f32::from_bits(0x5f000000) {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let mut state = Context::new(Input::cosine(angle));
    state.scalar = factor;
    let mut saved = SavedEnvironment([0; SAVED_BYTES]);
    // SAFETY: disjoint live locals hold the aligned architecture-sized save
    // image and every scalar. The last instruction restores the environment.
    unsafe {
        asm!(
            save_environment!(), "fldcw word ptr [{state}+0]",
            "fld dword ptr [{state}+12]", "fcos", "fmul dword ptr [{state}+4]",
            "fstp dword ptr [{state}+12]", "fnstsw word ptr [{state}+72]", restore_environment!(),
            state=in(reg)&mut state,saved=in(reg)&mut saved,options(nostack),
        );
    }
    state.record_precision();
    number(state.cosine)
}

const _: () = {
    assert!(std::mem::offset_of!(Context, control) == 0);
    assert!(std::mem::offset_of!(Context, scalar) == 4);
    assert!(std::mem::offset_of!(Context, mode) == 8);
    assert!(std::mem::offset_of!(Context, cosine) == 12);
    assert!(std::mem::offset_of!(Context, narrowed) == 24);
    assert!(std::mem::offset_of!(Context, first) == 32);
    assert!(std::mem::offset_of!(Context, second) == 44);
    assert!(std::mem::offset_of!(Context, squared) == 56);
    assert!(std::mem::offset_of!(Context, cutoff) == 60);
    assert!(std::mem::offset_of!(Context, epsilon) == 64);
    assert!(std::mem::offset_of!(Context, status) == 72);
    assert!(std::mem::offset_of!(Context, negative_one) == 76);
    assert!(std::mem::offset_of!(Context, incoming_precision) == 80);
};

#[repr(C, align(16))]
struct AxisAngleContext {
    control: u16,
    status: u16,
    angle: f32,
    axis: [f32; 3],
    result: [f32; 4],
    norm: f32,
    half: f32,
    one: f32,
    cutoff: f32,
    reserved: u32,
    epsilon: f64,
}

/// Original S_quat::Make(axis,angle), LS3DF RVA0x2ee00. FCOS remains in ST0
/// after its binary32 output store and feeds the original norm calculation;
/// FSIN remains retained through each axis product. No binary is loaded.
pub(crate) fn try_make_axis_angle(axis: [f32; 3], angle: f32) -> Result<[f32; 4], MakeUnavailable> {
    for value in axis.into_iter().chain([angle]) {
        number(value)?;
    }
    // The original feeds angle*0.5 directly to FSIN/FCOS. Their supported
    // magnitude is strictly below 2^63; preserve the raw argument and reject
    // the first binary32 angle whose half reaches that hardware boundary.
    if angle.abs() >= f32::from_bits(0x5f80_0000) {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let mut state = AxisAngleContext {
        control: 0x007f,
        status: 0,
        angle,
        axis,
        result: [0.; 4],
        norm: 0.,
        half: 0.5,
        one: 1.,
        cutoff: 1e-8,
        reserved: 0,
        epsilon: 1e-8,
    };
    let mut saved = SavedEnvironment([0; SAVED_BYTES]);
    // SAFETY: both aligned locals remain live and disjoint for the complete
    // block. The x87 sequence follows the original instruction order, uses
    // at most seven registers, and restores every incoming FP/SIMD register,
    // tag/control/status bit and instruction/data pointer before returning.
    unsafe {
        asm!(
            save_environment!(),
            "fldcw word ptr [{state}+0]",
            "fld dword ptr [{state}+4]", "fmul dword ptr [{state}+40]",
            "fld st(0)", "fcos", "fld st(0)", "fstp dword ptr [{state}+20]",
            "fxch st(1)", "fsin",
            "fld st(0)", "fmul dword ptr [{state}+8]", "fstp dword ptr [{state}+24]",
            "fld st(0)", "fmul dword ptr [{state}+12]", "fstp dword ptr [{state}+28]",
            "fmul dword ptr [{state}+16]", "fstp dword ptr [{state}+32]",
            "fld dword ptr [{state}+32]", "fld dword ptr [{state}+28]", "fld dword ptr [{state}+24]",
            "fld st(0)", "fmul st(0), st(1)",
            "fld st(2)", "fmul st(0), st(3)", "faddp st(1), st(0)",
            "fld st(3)", "fmul st(0), st(4)", "faddp st(1), st(0)",
            "fld st(4)", "fmul st(0), st(5)", "faddp st(1), st(0)",
            "fstp dword ptr [{state}+36]", "fstp st(0)", "fstp st(0)", "fstp st(0)", "fstp st(0)",
            "fld dword ptr [{state}+36]", "fsub dword ptr [{state}+44]", "fabs",
            "fcomp qword ptr [{state}+56]", "fnstsw ax", "test ah, 5", "jnp 4f",
            "fld dword ptr [{state}+36]", "fcomp dword ptr [{state}+48]", "fnstsw ax", "test ah, 5", "jp 3f",
            "mov dword ptr [{state}+24], 0", "mov dword ptr [{state}+28], 0", "mov dword ptr [{state}+32], 0",
            "mov dword ptr [{state}+20], 0x3f800000", "jmp 4f",
            "3:", "fld dword ptr [{state}+36]", "fsqrt", "fdivr dword ptr [{state}+44]",
            "fld st(0)", "fmul dword ptr [{state}+24]", "fstp dword ptr [{state}+24]",
            "fld st(0)", "fmul dword ptr [{state}+28]", "fstp dword ptr [{state}+28]",
            "fld st(0)", "fmul dword ptr [{state}+32]", "fstp dword ptr [{state}+32]",
            "fmul dword ptr [{state}+20]", "fstp dword ptr [{state}+20]",
            "4:", "fnstsw word ptr [{state}+2]",
            restore_environment!(),
            state=in(reg)&mut state,saved=in(reg)&mut saved,out("eax")_,options(nostack),
        );
    }
    crate::native_precision::record_precision(state.status & 0x20 != 0);
    if state.status & 0x1f != 0 {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    number(state.norm)?;
    for value in state.result {
        number(value)?;
    }
    Ok(state.result)
}
const _: () = {
    assert!(std::mem::offset_of!(AxisAngleContext, control) == 0);
    assert!(std::mem::offset_of!(AxisAngleContext, status) == 2);
    assert!(std::mem::offset_of!(AxisAngleContext, angle) == 4);
    assert!(std::mem::offset_of!(AxisAngleContext, axis) == 8);
    assert!(std::mem::offset_of!(AxisAngleContext, result) == 20);
    assert!(std::mem::offset_of!(AxisAngleContext, norm) == 36);
    assert!(std::mem::offset_of!(AxisAngleContext, half) == 40);
    assert!(std::mem::offset_of!(AxisAngleContext, one) == 44);
    assert!(std::mem::offset_of!(AxisAngleContext, cutoff) == 48);
    assert!(std::mem::offset_of!(AxisAngleContext, epsilon) == 56);
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_zero_products_keep_original_crt_cosine_sign() {
        let values = try_acos_sin_cos_products(0., 0., 0.).unwrap();
        assert_eq!(values[0].to_bits(), 0);
        assert_eq!(values[1].to_bits(), 0);
        assert_eq!(
            try_vector_angle_sine_scaled([1., 0., 0.], [0., 1., 0.], [2., -3., 0.]).unwrap(),
            [2., -3., 0.]
        );
        assert_eq!(try_vector_angle_to([1., 0., 0.], [1., 0., 0.]).unwrap(), 0.);
        assert_eq!(
            try_vector_angle_to([1., 0., 0.], [0., 1., 0.]).unwrap(),
            std::f32::consts::FRAC_PI_2
        );
        assert_eq!(try_cosine_product(0., 1.5).unwrap(), 1.5);
        assert_eq!(try_cosine_product(std::f32::consts::TAU, 2.).unwrap(), 2.);
        assert_eq!(
            try_vector_angle_sine([1., 0., 0.], [1., 0., 0.])
                .unwrap()
                .to_bits(),
            0
        );
        assert_eq!(
            try_vector_angle_sine([1., 0., 0.], [0., 1., 0.]).unwrap(),
            1.
        );
    }
    #[test]
    fn engine_divides_by_retained_angle_after_independent_binary32_spill() {
        let angle = try_retained_vector_angle([1., 0., 0.], [0., 1., 0.]).unwrap();
        assert_eq!(angle.spill().unwrap().to_bits(), 0x3fc90fdb);
        assert_eq!(angle.divide_numerator(1.7).unwrap().to_bits(), 0x3f8a874a);
        assert_eq!((1.7_f32 / angle.spill().unwrap()).to_bits(), 0x3f8a8749);
        // Repeated binary32 stores leave the original retained value intact.
        assert_eq!(angle.spill().unwrap().to_bits(), 0x3fc90fdb);
        assert_eq!(angle.divide_numerator(-1.7).unwrap().to_bits(), 0xbf8a874a);
    }

    #[test]
    fn scoped_precision_overrides_ambient_and_retains_calculation_history() {
        let mut saved = SavedEnvironment([0; SAVED_BYTES]);
        let control = 0x007f_u16;
        let three = 3_f32;
        // SAFETY: establish ambient PE, then restore the incoming complete
        // hardware state before any assertion. The final standalone restore
        // declares ABI clobbers because it replaces XMM values across Rust.
        unsafe {
            asm!(save_environment!(), "fldcw word ptr [{control}]",
                "fld1", "fdiv dword ptr [{three}]", "fstp st(0)",
                saved=in(reg)&mut saved,control=in(reg)&control,three=in(reg)&three,options(nostack));
        }
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        let clean = precision.run(|| try_vector_angle_sine([1., 0., 0.], [-1., 0., 0.]));
        let sticky = precision.run(|| try_vector_angle_sine([1., 0., 0.], [-1., 0., 0.]));
        let mut token_precision = crate::native_precision::Pc24PrecisionState::default();
        let token = token_precision.run(|| try_retained_vector_angle([1., 0., 0.], [-1., 0., 0.]));
        let token_compute_raised = token_precision.precision_raised();
        let spill = token_precision.run(|| token.and_then(|value| value.spill()));
        let mut division_precision = crate::native_precision::Pc24PrecisionState::default();
        let division = division_precision
            .run(|| try_retained_vector_angle([1., 0., 0.], [-1., 0., 0.])?.divide_numerator(1.7));
        let mut computation_precision = crate::native_precision::Pc24PrecisionState::default();
        let tiny = f32::from_bits(0x30800000);
        let computation = computation_precision
            .run(|| try_retained_vector_angle([1., tiny, 0.], [-1., -tiny, 0.]));
        let mut cosine_precision = crate::native_precision::Pc24PrecisionState::default();
        let cosine = cosine_precision.run(|| try_cosine_product(1., 1.));
        unsafe {
            asm!(restore_environment!(),saved=in(reg)&saved,clobber_abi("C"),options(nostack));
        }
        assert_eq!(clean.unwrap().to_bits(), 0x9f800000);
        assert_eq!(sticky.unwrap().to_bits(), 0x250d3000);
        assert!(precision.precision_raised());
        assert!(!token_compute_raised);
        assert_eq!(spill.unwrap().to_bits(), 0x40490fdb);
        assert!(token_precision.precision_raised());
        assert!(division.is_ok());
        assert!(division_precision.precision_raised());
        assert!(computation.is_ok());
        assert!(computation_precision.precision_raised());
        assert!(cosine.is_ok());
        assert!(cosine_precision.precision_raised());
    }

    #[cfg(target_arch = "x86")]
    #[test]
    fn preserves_control_status_tags_and_live_register_stack() {
        let mut caller = [0_u8; 108];
        let mut before = [0_u8; 108];
        let mut after = [0_u8; 108];
        let control = 0x0b7f_u16;
        let values = [2.5_f64, -7.25, 3.];
        // SAFETY: build a nondefault, masked caller environment with three live
        // registers and a pending invalid-operation status flag. The original
        // test-thread state is restored before any Rust assertion can unwind.
        unsafe {
            asm!(
                "fnsave [{caller}]",
                "fldcw word ptr [{control}]",
                "fldz","fldz","fdivp st(1), st(0)","fstp st(0)",
                "fld qword ptr [{values}]","fld qword ptr [{values}+8]","fld qword ptr [{values}+16]",
                "fnsave [{before}]","frstor [{before}]",
                caller=in(reg)caller.as_mut_ptr(),before=in(reg)before.as_mut_ptr(),control=in(reg)&control,values=in(reg)values.as_ptr(),options(nostack),
            );
        }
        let result = try_wheel_friction_limit([3., 0.5, -2.], [1., 0., 0.], 30., 20.);
        let scaled = try_vector_angle_sine_scaled([1., 0., 0.], [0., 1., 0.], [2., -3., 0.]);
        let cosine = try_cosine_product(100., 0.05);
        let quaternion = try_make_axis_angle([0.25, 0., -1.], f32::from_bits((127 - 44) << 23));
        let retained = try_retained_vector_angle([1., 0., 0.], [0., 1., 0.]);
        let (spill, quotient, comparison) = match retained {
            Ok(value) => (
                value.spill(),
                value.divide_numerator(1.7),
                value.compare(0.78),
            ),
            Err(error) => (Err(error), Err(error), Err(error)),
        };
        unsafe {
            asm!("fnsave [{after}]","frstor [{caller}]",after=in(reg)after.as_mut_ptr(),caller=in(reg)caller.as_ptr(),clobber_abi("C"),options(nostack));
        }
        assert!(result.is_ok());
        assert!(scaled.is_ok());
        assert!(cosine.is_ok());
        assert!(quaternion.is_ok());
        assert!(spill.is_ok());
        assert!(quotient.is_ok());
        assert_eq!(comparison, Ok(std::cmp::Ordering::Greater));
        assert_eq!(before, after);
    }
    #[cfg(target_arch = "x86")]
    #[test]
    fn pending_unmasked_exception_is_saved_without_waiting() {
        let mut caller = [0_u8; 108];
        let mut before = [0_u8; 108];
        let mut after = [0_u8; 108];
        unsafe {
            asm!(
                "fnsave [{caller}]", "fldz", "fldz", "fdivp st(1), st(0)", "fstp st(0)",
                "fnsave [{before}]",
                caller=in(reg)caller.as_mut_ptr(),before=in(reg)before.as_mut_ptr(),options(nostack),
            );
        }
        // A saved unmasked pending invalid operation must survive, but must
        // not trap on entry to these isolated CPU expressions.
        before[0] &= !1;
        let status = u16::from_le_bytes([before[4], before[5]]) | 0x8080;
        before[4..6].copy_from_slice(&status.to_le_bytes());
        unsafe {
            asm!("frstor [{before}]",before=in(reg)before.as_ptr(),options(nostack));
        }
        let result = try_acos_sin_cos_products(0.25, 2., 3.);
        unsafe {
            asm!("fnsave [{after}]","frstor [{caller}]",after=in(reg)after.as_mut_ptr(),caller=in(reg)caller.as_ptr(),clobber_abi("C"),options(nostack));
        }
        assert!(result.is_ok());
        assert_eq!(before, after);
    }
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn preserves_long_mode_x87_pointers_across_actual_helpers() {
        let mut caller = SavedEnvironment([0; SAVED_BYTES]);
        let mut before = SavedEnvironment([0; SAVED_BYTES]);
        let mut after = SavedEnvironment([0; SAVED_BYTES]);
        let control = 0x0b7f_u16;
        let values = [2.5_f64, -7.25, 3.];
        // SAFETY: every memory image is aligned and complete. Capturing with
        // FXSAVE64 is nonwaiting and leaves the live caller state unchanged.
        // Keep a pending unmasked invalid exception across actual Rust helpers.
        unsafe {
            asm!(
                "fxsave64 [{caller}]", "fninit", "fldcw word ptr [{control}]",
                "fldz", "fldz", "fdivp st(1), st(0)", "fstp st(0)",
                "fld qword ptr [{values}]", "fld qword ptr [{values}+8]", "fld qword ptr [{values}+16]",
                "fxsave64 [{before}]",
                // Some CPUs report zero FDP after nonfaulting loads. Seed a
                // valid caller data pointer through its architectural image.
                "mov qword ptr [{before}+16], {values}",
                "and word ptr [{before}], 0xfffe", "or word ptr [{before}+2], 0x8080",
                "fxrstor64 [{before}]", "fxsave64 [{before}]",
                caller=in(reg)&mut caller,before=in(reg)&mut before,control=in(reg)&control,values=in(reg)values.as_ptr(),options(nostack),
            );
        }
        let result = try_wheel_friction_limit([3., 0.5, -2.], [1., 0., 0.], 30., 20.);
        let scaled = try_vector_angle_sine_scaled([1., 0., 0.], [0., 1., 0.], [2., -3., 0.]);
        let cosine = try_cosine_product(100., 0.05);
        let quaternion = try_make_axis_angle([0.25, 0., -1.], f32::from_bits((127 - 44) << 23));
        let retained = try_retained_vector_angle([1., 0., 0.], [0., 1., 0.]);
        let (spill, quotient, comparison) = match retained {
            Ok(value) => (
                value.spill(),
                value.divide_numerator(1.7),
                value.compare(0.78),
            ),
            Err(error) => (Err(error), Err(error), Err(error)),
        };
        unsafe {
            asm!("fxsave64 [{after}]", "fxrstor64 [{caller}]",after=in(reg)&mut after,caller=in(reg)&caller,clobber_abi("C"),options(nostack));
        }
        assert!(result.is_ok());
        assert!(scaled.is_ok());
        assert!(cosine.is_ok());
        assert!(quaternion.is_ok());
        assert!(spill.is_ok());
        assert!(quotient.is_ok());
        assert_eq!(comparison, Ok(std::cmp::Ordering::Greater));
        let fip = u64::from_le_bytes(before.0[8..16].try_into().unwrap());
        let fdp = u64::from_le_bytes(before.0[16..24].try_into().unwrap());
        assert!(fip > u64::from(u32::MAX));
        assert!(fdp > u64::from(u32::MAX));
        // Ordinary Rust calls may use caller-saved XMM registers/MXCSR flags.
        // This comparison covers full x87 state, including the high pointers.
        assert_eq!(&before.0[..24], &after.0[..24]);
        assert_eq!(&before.0[32..160], &after.0[32..160]);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn preserves_full_fxsave64_state_with_pending_exception_and_live_sse() {
        let mut caller = SavedEnvironment([0; SAVED_BYTES]);
        let mut before = SavedEnvironment([0; SAVED_BYTES]);
        let mut after = SavedEnvironment([0; SAVED_BYTES]);
        let mut saved = SavedEnvironment([0; SAVED_BYTES]);
        let control = 0x007f_u16;
        let values = [2.5_f64, -7.25, 3.];
        let pattern = std::array::from_fn::<_, 256, _>(|i| (i as u8).wrapping_mul(37));
        let caller_mxcsr = 0x5f81_u32;
        let work_mxcsr = 0x1f80_u32;
        // SAFETY: use the production save/restore instructions in one block,
        // so compiler-generated scalar/SIMD instructions cannot intervene.
        // A real memory FLD supplies the instruction pointer; the data
        // pointer is explicitly seeded where nonfaulting loads leave it zero. The
        // pending unmasked invalid operation is restored without any FWAIT.
        // The test deliberately changes SSE state inside the isolated block
        // to verify that the full captured image, not only x87, is restored.
        unsafe {
            asm!(
                "fxsave64 [{caller}]", "fninit", "fldcw word ptr [{control}]",
                "fldz", "fldz", "fdivp st(1), st(0)", "fstp st(0)",
                "fld qword ptr [{values}]", "fld qword ptr [{values}+8]", "fld qword ptr [{values}+16]",
                "ldmxcsr dword ptr [{caller_mxcsr}]",
                "movdqu xmm0, [{pattern}]", "movdqu xmm1, [{pattern}+16]",
                "movdqu xmm2, [{pattern}+32]", "movdqu xmm3, [{pattern}+48]",
                "movdqu xmm4, [{pattern}+64]", "movdqu xmm5, [{pattern}+80]",
                "movdqu xmm6, [{pattern}+96]", "movdqu xmm7, [{pattern}+112]",
                "movdqu xmm8, [{pattern}+128]", "movdqu xmm9, [{pattern}+144]",
                "movdqu xmm10, [{pattern}+160]", "movdqu xmm11, [{pattern}+176]",
                "movdqu xmm12, [{pattern}+192]", "movdqu xmm13, [{pattern}+208]",
                "movdqu xmm14, [{pattern}+224]", "movdqu xmm15, [{pattern}+240]",
                "fxsave64 [{before}]",
                "mov qword ptr [{before}+16], {values}",
                "and word ptr [{before}], 0xfffe", "or word ptr [{before}+2], 0x8080",
                "fxrstor64 [{before}]", "fxsave64 [{before}]",
                save_environment!(), "fldcw word ptr [{control}]",
                "fld1", "fld1", "faddp st(1), st(0)",
                "pxor xmm0, xmm0", "pxor xmm15, xmm15", "ldmxcsr dword ptr [{work_mxcsr}]",
                restore_environment!(), "fxsave64 [{after}]", "fxrstor64 [{caller}]",
                caller=in(reg)&mut caller,before=in(reg)&mut before,after=in(reg)&mut after,saved=in(reg)&mut saved,
                control=in(reg)&control,values=in(reg)values.as_ptr(),pattern=in(reg)pattern.as_ptr(),
                caller_mxcsr=in(reg)&caller_mxcsr,work_mxcsr=in(reg)&work_mxcsr,options(nostack),
            );
        }
        assert!(u64::from_le_bytes(before.0[8..16].try_into().unwrap()) > u64::from(u32::MAX));
        assert!(u64::from_le_bytes(before.0[16..24].try_into().unwrap()) > u64::from(u32::MAX));
        assert_eq!(
            u32::from_le_bytes(before.0[24..28].try_into().unwrap()),
            caller_mxcsr
        );
        assert_eq!(&before.0[160..416], &pattern);
        assert_eq!(before.0, after.0);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn isolated_save_restore_leaves_avx_upper_lanes_unchanged() {
        if !std::is_x86_feature_detected!("avx") {
            return;
        }
        let mut saved = SavedEnvironment([0; SAVED_BYTES]);
        let mut caller_ymm = [0_u8; 32];
        let mut after = [0_u8; 32];
        let pattern = std::array::from_fn::<_, 32, _>(|i| (i as u8).wrapping_mul(53));
        // SAFETY: AVX/OS support is checked above. Preserve the complete
        // original YMM15 in addition to the production FXSAVE64 state.
        unsafe {
            asm!(
                "vmovdqu [{caller_ymm}], ymm15", "vmovdqu ymm15, [{pattern}]",
                save_environment!(), "fld1", "fldpi", "faddp st(1), st(0)",
                restore_environment!(), "vmovdqu [{after}], ymm15", "vmovdqu ymm15, [{caller_ymm}]",
                saved=in(reg)&mut saved,caller_ymm=in(reg)caller_ymm.as_mut_ptr(),
                pattern=in(reg)pattern.as_ptr(),after=in(reg)after.as_mut_ptr(),options(nostack),
            );
        }
        assert_eq!(pattern, after);
    }

    #[test]
    fn tiny_antiparallel_components_set_real_pc24_precision_sticky() {
        let mut saved = SavedEnvironment([0; SAVED_BYTES]);
        let control = 0x007f_u16;
        let mut rows = Vec::new();
        for exponent in [10_u32, 15, 20, 25, 30, 40, 50, 60] {
            let tiny = f32::from_bits((127 - exponent) << 23);
            // Establish a genuinely clean caller status. Restore it before
            // assertions, including on a helper availability error.
            unsafe {
                asm!(save_environment!(),"fldcw word ptr [{control}]",saved=in(reg)&mut saved,control=in(reg)&control,options(nostack));
            }
            let sine = try_vector_angle_sine([1., tiny, 0.], [-1., -tiny, 0.]);
            let scaled =
                try_vector_angle_sine_scaled([1., tiny, 0.], [-1., -tiny, 0.], [1., 2., -0.5]);
            unsafe {
                asm!(restore_environment!(),saved=in(reg)&saved,clobber_abi("C"),options(nostack));
            }
            rows.push((exponent, sine, scaled));
        }
        for (exponent, sine, scaled) in rows {
            assert_eq!(sine.unwrap().to_bits(), 0x250d3000, "2^-{exponent}");
            assert_eq!(
                scaled.unwrap().map(f32::to_bits),
                [0x250d3000, 0x258d3000, 0xa48d3000],
                "2^-{exponent}"
            );
        }
    }
}
