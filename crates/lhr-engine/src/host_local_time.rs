//! Host adapter for the local wall-clock read of the HUD constructor.
//!
//! `G_IndicatorsClass::ResetValues` (GAME 0x5f6cf0, mafia-re
//! game/ui/g_indicators.cpp:162-164 at pin fe7e746) calls `GetLocalTime`
//! and passes `wHour`/`wMinute`/`wSecond` to `TimerSetTime`. The session's
//! `mafiaIndicators` owner takes that read as a staged
//! [`TimerClock`]; this module produces it on the host.
//!
//! FFI boundary (the only unsafe code here): std exposes UTC but no local
//! time zone, and a time-zone crate would be a new dependency for one read.
//! The platform layer asks the OS for the current UTC offset including
//! daylight saving (`GetTimeZoneInformation` on Windows, `localtime_r`'s
//! `tm_gmtoff` on Unix, both from crates already in Cargo.lock); the pure
//! [`timer_clock_at`] folds UTC seconds plus that offset into the
//! wall-clock words. Each unsafe block passes a pointer to a local,
//! zero-initialized, correctly sized out-struct that outlives the call; no
//! pointer is retained, and failure is reported instead of a default.

use lhr_engine::indicators::TimerClock;

/// Wall clock at `utc_seconds` (seconds since the Unix epoch) for a zone
/// `offset_seconds` east of UTC: the day's seconds wrap at midnight in both
/// directions.
pub(crate) fn timer_clock_at(utc_seconds: i64, offset_seconds: i32) -> TimerClock {
    let local = (utc_seconds + i64::from(offset_seconds)).rem_euclid(86_400) as u32;
    TimerClock {
        hour: local / 3600,
        minute: local / 60 % 60,
        second: local % 60,
    }
}

/// The host's current local wall clock, the stand-in for `GetLocalTime`.
pub(crate) fn local_timer_clock() -> Result<TimerClock, String> {
    let utc_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("host clock before the Unix epoch: {error}"))?
        .as_secs();
    let utc_seconds = i64::try_from(utc_seconds)
        .map_err(|_| "host clock beyond the representable range".to_string())?;
    let offset = platform::utc_offset_seconds(utc_seconds)?;
    Ok(timer_clock_at(utc_seconds, offset))
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::Time::{
        GetTimeZoneInformation, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    };

    /// `TIME_ZONE_ID_STANDARD`/`TIME_ZONE_ID_DAYLIGHT` (winnt.h); the
    /// windows-sys constants live behind an unrelated feature.
    const TIME_ZONE_ID_STANDARD: u32 = 1;
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

    /// Seconds east of UTC now: `-(Bias + StandardBias|DaylightBias)`
    /// minutes for the period the OS reports as current.
    pub(super) fn utc_offset_seconds(_utc_seconds: i64) -> Result<i32, String> {
        let mut info = std::mem::MaybeUninit::<TIME_ZONE_INFORMATION>::zeroed();
        // SAFETY: `info` is a live, zeroed, properly aligned local of the exact
        // type the API writes; the pointer is not retained after the call.
        let id = unsafe { GetTimeZoneInformation(info.as_mut_ptr()) };
        if id == TIME_ZONE_ID_INVALID {
            return Err(format!(
                "GetTimeZoneInformation failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: the struct is plain data, zero-initialized above and filled
        // by the successful call, so every bit pattern in it is valid.
        let info = unsafe { info.assume_init() };
        let bias = match id {
            TIME_ZONE_ID_DAYLIGHT => info.Bias + info.DaylightBias,
            TIME_ZONE_ID_STANDARD => info.Bias + info.StandardBias,
            _ => info.Bias,
        };
        bias.checked_mul(-60)
            .ok_or_else(|| format!("time-zone bias {bias} out of range"))
    }
}

#[cfg(unix)]
mod platform {
    /// Seconds east of UTC at `utc_seconds` from `localtime_r`'s
    /// `tm_gmtoff`, which includes daylight saving.
    pub(super) fn utc_offset_seconds(utc_seconds: i64) -> Result<i32, String> {
        let time = libc::time_t::try_from(utc_seconds)
            .map_err(|_| "host clock beyond time_t".to_string())?;
        let mut tm = std::mem::MaybeUninit::<libc::tm>::zeroed();
        // SAFETY: both pointers name live, properly aligned locals of the exact
        // types `localtime_r` reads and writes; neither is retained.
        let result = unsafe { libc::localtime_r(&time, tm.as_mut_ptr()) };
        if result.is_null() {
            return Err(format!(
                "localtime_r failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: zero-initialized plain data filled by the successful call;
        // the only pointer member (`tm_zone`) is never dereferenced here.
        let tm = unsafe { tm.assume_init() };
        i32::try_from(tm.tm_gmtoff).map_err(|_| format!("UTC offset {} out of range", tm.tm_gmtoff))
    }
}

#[cfg(not(any(windows, unix)))]
mod platform {
    pub(super) fn utc_offset_seconds(_utc_seconds: i64) -> Result<i32, String> {
        Err("no host local-time adapter on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(hour: u32, minute: u32, second: u32) -> TimerClock {
        TimerClock {
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn local_clock_applies_non_zero_offsets_and_wraps_midnight() {
        // 2026-01-15 08:00:00 UTC: Prague in winter (UTC+1) reads 09:00:00.
        assert_eq!(timer_clock_at(1_768_464_000, 3600), clock(9, 0, 0));
        assert_eq!(timer_clock_at(1_768_464_000, 0), clock(8, 0, 0));
        // 2026-07-15 07:00:00 UTC: Prague in summer (UTC+2) reads 09:00:00.
        assert_eq!(timer_clock_at(1_784_098_800, 7200), clock(9, 0, 0));
        // 03:30:00 UTC at UTC-5 is the previous evening.
        assert_eq!(timer_clock_at(1_768_447_800, -18_000), clock(22, 30, 0));
        // 23:30:00 UTC at UTC+1 is past midnight; half-hour zones keep minutes.
        assert_eq!(timer_clock_at(1_768_519_800, 3600), clock(0, 30, 0));
        assert_eq!(timer_clock_at(1_768_519_800, 19_800), clock(5, 0, 0));
        assert_eq!(timer_clock_at(1_768_464_007, 3600), clock(9, 0, 7));
    }

    /// The adapter agrees with the native `GetLocalTime` read it stands in
    /// for, whatever the machine's zone and daylight-saving state.
    #[cfg(windows)]
    #[test]
    fn windows_adapter_matches_get_local_time() {
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        fn native() -> TimerClock {
            let mut time = std::mem::MaybeUninit::zeroed();
            // SAFETY: live, zeroed, aligned SYSTEMTIME local; not retained.
            unsafe { GetLocalTime(time.as_mut_ptr()) };
            // SAFETY: plain data filled by the infallible call.
            let time: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { time.assume_init() };
            clock(
                u32::from(time.wHour),
                u32::from(time.wMinute),
                u32::from(time.wSecond),
            )
        }
        // Retry across a second boundary between the two reads.
        for _ in 0..3 {
            let before = native();
            let adapter = local_timer_clock().unwrap();
            let after = native();
            if adapter == before || adapter == after {
                return;
            }
        }
        panic!("host adapter disagrees with GetLocalTime");
    }
}
