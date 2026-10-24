//! Scoped precision-exception state for the original PC24 arithmetic pipeline.
//!
//! The CRT consults sticky x87 precision when deciding whether to narrow an
//! acos result to binary64. Rust SSE arithmetic does not set that x87 bit.
//! This host state follows the original arithmetic without changing the host
//! thread's hardware environment. Other FP status/control bits are not modeled.
use std::cell::Cell;
thread_local! { static ACTIVE: Cell<Option<bool>> = const { Cell::new(None) }; }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pc24PrecisionState {
    raised: bool,
}
impl Pc24PrecisionState {
    pub fn new(raised: bool) -> Self {
        Self { raised }
    }
    pub fn precision_raised(self) -> bool {
        self.raised
    }
    /// Runs one Game owner's arithmetic with its persistent precision state.
    /// Independent nested owners restore the enclosing context on every exit.
    pub fn run<R>(&mut self, operation: impl FnOnce() -> R) -> R {
        struct Scope<'a> {
            owner: &'a mut Pc24PrecisionState,
            previous: Option<bool>,
        }
        impl Drop for Scope<'_> {
            fn drop(&mut self) {
                self.owner.raised = ACTIVE.with(|active| active.replace(self.previous)).unwrap();
            }
        }
        let previous = ACTIVE.with(|active| active.replace(Some(self.raised)));
        let _scope = Scope {
            owner: self,
            previous,
        };
        operation()
    }
    /// Scopes arithmetic while the operation also mutably borrows its owner.
    /// The selected state must not be replaced during the operation. Unwinding
    /// retains the reached precision prefix just like `run`.
    /// Callers must use their inner methods when reentering the same owner;
    /// recursively scoping it would reload its state from before this call.
    pub(crate) fn run_owner<T, R>(
        owner: &mut T,
        state: fn(&mut T) -> &mut Self,
        operation: impl FnOnce(&mut T) -> R,
    ) -> R {
        struct Scope<'a, T> {
            owner: &'a mut T,
            state: fn(&mut T) -> &mut Pc24PrecisionState,
            previous: Option<bool>,
        }
        impl<T> Drop for Scope<'_, T> {
            fn drop(&mut self) {
                (self.state)(self.owner).raised =
                    ACTIVE.with(|active| active.replace(self.previous)).unwrap();
            }
        }
        let raised = state(owner).raised;
        let previous = ACTIVE.with(|active| active.replace(Some(raised)));
        let scope = Scope {
            owner,
            state,
            previous,
        };
        operation(scope.owner)
    }
}
pub(crate) fn current_precision() -> Option<bool> {
    ACTIVE.with(Cell::get)
}
pub(crate) fn record_precision(raised: bool) {
    if raised {
        ACTIVE.with(|active| {
            if active.get().is_some() {
                active.set(Some(true));
            }
        });
    }
}

#[derive(Clone, Copy)]
struct Dyadic {
    negative: bool,
    significand: u64,
    exponent: i32,
}
fn value(x: f32) -> Option<Dyadic> {
    let bits = x.to_bits();
    let exponent = (bits >> 23) & 255;
    let fraction = bits & 0x7fffff;
    if exponent == 255 || exponent == 0 && fraction != 0 {
        return None;
    }
    Some(Dyadic {
        negative: bits >> 31 != 0,
        significand: if exponent == 0 {
            0
        } else {
            u64::from(fraction | 0x800000)
        },
        exponent: exponent as i32 - 150,
    })
}
fn canonical(mut x: Dyadic) -> Dyadic {
    if x.significand == 0 {
        x.negative = false;
        x.exponent = 0;
    } else {
        let zeros = x.significand.trailing_zeros();
        x.significand >>= zeros;
        x.exponent += zeros as i32;
    }
    x
}
fn equal(a: Dyadic, b: Dyadic) -> bool {
    let (a, b) = (canonical(a), canonical(b));
    a.negative == b.negative && a.significand == b.significand && a.exponent == b.exponent
}
fn product(a: Dyadic, b: Dyadic) -> Dyadic {
    Dyadic {
        negative: a.negative ^ b.negative,
        significand: a.significand * b.significand,
        exponent: a.exponent + b.exponent,
    }
}
// Wider-exponent PC24 values (including binary32 subnormal spills) remain
// exact dyadics with at most 24 significant bits. Decode their operands before
// any host f64 sum: 1 + 2^-60 already loses its smaller term in that sum.
fn wide_pc24_value(x: f64) -> Option<Dyadic> {
    let bits = x.to_bits();
    let exponent = (bits >> 52) & 2047;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 2047 {
        return None;
    }
    let value = canonical(Dyadic {
        negative: bits >> 63 != 0,
        significand: if exponent == 0 {
            fraction
        } else {
            fraction | (1_u64 << 52)
        },
        exponent: if exponent == 0 {
            -1074
        } else {
            exponent as i32 - 1075
        },
    });
    (value.significand <= 0x00ff_ffff).then_some(value)
}
fn exact_dyadic_sum(a: Dyadic, b: Dyadic, r: Dyadic) -> bool {
    if a.significand == 0 {
        return equal(b, r);
    }
    if b.significand == 0 {
        return equal(a, r);
    }
    // At this separation the smaller nonzero operand lies strictly below one
    // ulp of the larger. Its contribution cannot disappear in an exact sum.
    if (a.exponent - b.exponent).abs() > 64 {
        return false;
    }
    let minimum = if r.significand == 0 {
        a.exponent.min(b.exponent)
    } else {
        a.exponent.min(b.exponent).min(r.exponent)
    };
    let integer = |x: Dyadic| -> Option<i128> {
        if x.significand == 0 {
            return Some(0);
        }
        let shift = u32::try_from(x.exponent - minimum).ok()?;
        let width = 64 - x.significand.leading_zeros();
        if shift.checked_add(width)? > 127 {
            return None;
        }
        let v = i128::from(x.significand) << shift;
        Some(if x.negative { -v } else { v })
    };
    match (integer(a), integer(b), integer(r)) {
        (Some(a), Some(b), Some(r)) => a.checked_add(b) == Some(r),
        _ => false,
    }
}
fn exact_sum(a: f32, b: f32, result: f32) -> bool {
    match (value(a), value(b), value(result)) {
        (Some(a), Some(b), Some(r)) => exact_dyadic_sum(a, b, r),
        _ => false,
    }
}
pub(crate) fn record_wide_pc24_add(a: f64, b: f64, result: f64) {
    record(|| {
        match (
            wide_pc24_value(a),
            wide_pc24_value(b),
            wide_pc24_value(result),
        ) {
            (Some(a), Some(b), Some(r)) => exact_dyadic_sum(a, b, r),
            _ => false,
        }
    });
}
pub(crate) fn record_wide_pc24_sub(a: f64, b: f64, result: f64) {
    record_wide_pc24_add(a, -b, result);
}
pub(crate) fn record_wide_pc24_mul(a: f64, b: f64, result: f64) {
    record(|| {
        match (
            wide_pc24_value(a),
            wide_pc24_value(b),
            wide_pc24_value(result),
        ) {
            (Some(a), Some(b), Some(r)) => equal(product(a, b), r),
            _ => false,
        }
    });
}
fn record(exact: impl FnOnce() -> bool) {
    if current_precision() == Some(false) && !exact() {
        record_precision(true);
    }
}
pub(crate) fn record_add(a: f32, b: f32, result: f32) {
    record(|| exact_sum(a, b, result));
}
pub(crate) fn record_sub(a: f32, b: f32, result: f32) {
    record(|| exact_sum(a, -b, result));
}
pub(crate) fn record_mul(a: f32, b: f32, result: f32) {
    record(|| match (value(a), value(b), value(result)) {
        (Some(a), Some(b), Some(r)) => equal(product(a, b), r),
        _ => false,
    });
}
pub(crate) fn record_div(a: f32, b: f32, result: f32) {
    record(|| match (value(a), value(b), value(result)) {
        (Some(a), Some(b), Some(r)) if b.significand != 0 => equal(a, product(b, r)),
        _ => false,
    });
}
pub(crate) fn record_sqrt(a: f32, result: f32) {
    record(|| match (value(a), value(result)) {
        (Some(a), Some(r)) if !a.negative || a.significand == 0 => equal(a, product(r, r)),
        _ => false,
    });
}
pub(crate) fn add(a: f32, b: f32) -> f32 {
    let r = a + b;
    record_add(a, b, r);
    r
}
pub(crate) fn sub(a: f32, b: f32) -> f32 {
    let r = a - b;
    record_sub(a, b, r);
    r
}
pub(crate) fn mul(a: f32, b: f32) -> f32 {
    let r = a * b;
    record_mul(a, b, r);
    r
}
pub(crate) fn div(a: f32, b: f32) -> f32 {
    let r = a / b;
    record_div(a, b, r);
    r
}
pub(crate) fn sqrt(a: f32) -> f32 {
    let r = a.sqrt();
    record_sqrt(a, r);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wide_pc24_operands_preserve_exactness_beyond_binary64_sums() {
        let tiny = 2_f64.powi(-60);
        assert_eq!(1.0 + tiny, 1.0);
        for subtract in [false, true] {
            let mut state = Pc24PrecisionState::default();
            state.run(|| {
                if subtract {
                    record_wide_pc24_sub(1., tiny, 1.);
                } else {
                    record_wide_pc24_add(1., tiny, 1.);
                }
            });
            assert!(state.precision_raised());
        }
        let mut exact = Pc24PrecisionState::default();
        exact.run(|| {
            record_wide_pc24_mul(2_f64.powi(-200), 2_f64.powi(180), 2_f64.powi(-20));
            record_wide_pc24_add(2_f64.powi(200), -2_f64.powi(200), 0.);
            let subnormal = f64::from(f32::from_bits(1));
            record_wide_pc24_mul(subnormal, 2., subnormal * 2.);
            record_wide_pc24_add(subnormal, subnormal, subnormal * 2.);
        });
        assert!(!exact.precision_raised());
        let mut product = Pc24PrecisionState::default();
        product.run(|| {
            record_wide_pc24_mul(
                f64::from(0.4_f32),
                f64::from(0.2_f32),
                f64::from(0.4_f32 * 0.2_f32),
            )
        });
        assert!(product.precision_raised());
    }

    #[test]
    fn exact_dyadics_detect_precision_beyond_binary64() {
        let mut s = Pc24PrecisionState::default();
        s.run(|| {
            record_add(1., 2., 3.);
            record_mul(0.001, 16., 0.001 * 16.);
            record_div(1., 4., 0.25);
            record_sqrt(4., 2.);
        });
        assert!(!s.precision_raised());
        s.run(|| record_add(1., 2f32.powi(-60), 1.));
        assert!(s.precision_raised());
        for operation in [0, 1, 2] {
            let mut s = Pc24PrecisionState::default();
            s.run(|| match operation {
                0 => record_div(1., 3., 1. / 3.),
                1 => record_sqrt(2., 2f32.sqrt()),
                _ => record_mul(83., 0.001, 83. * 0.001),
            });
            assert!(s.precision_raised());
        }
    }
    #[test]
    fn cancellation_zero_and_nested_owner_state_are_preserved() {
        let mut outer = Pc24PrecisionState::default();
        outer.run(|| {
            record_sub(1., 1., 0.);
            record_add(1., -f32::from_bits(0x3f7fffff), 2f32.powi(-24));
            assert_eq!(current_precision(), Some(false));
            Pc24PrecisionState::new(true).run(|| assert_eq!(current_precision(), Some(true)));
            assert_eq!(current_precision(), Some(false));
        });
        assert!(!outer.precision_raised());
        assert_eq!(current_precision(), None);
    }
    #[test]
    fn unwinding_restores_the_enclosing_context_and_retains_the_prefix() {
        let mut state = Pc24PrecisionState::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            state.run(|| {
                record_div(1., 3., 1. / 3.);
                panic!("synthetic caller interruption");
            })
        }));
        assert!(result.is_err());
        assert!(state.precision_raised());
        assert_eq!(current_precision(), None);
    }
    #[test]
    fn mutable_owner_keeps_both_storage_and_precision_after_unwinding() {
        #[derive(Default)]
        struct Owner {
            precision: Pc24PrecisionState,
            reached: bool,
        }
        let mut owner = Owner::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Pc24PrecisionState::run_owner(
                &mut owner,
                |o| &mut o.precision,
                |o| {
                    o.reached = true;
                    record_div(1., 3., 1. / 3.);
                    panic!("synthetic owner interruption");
                },
            )
        }));
        assert!(result.is_err());
        assert!(owner.reached && owner.precision.precision_raised());
        assert_eq!(current_precision(), None);
    }
}
