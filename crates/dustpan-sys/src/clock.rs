//! [`SystemClock`]: the real clock core is handed.
use dustpan_core::time::{Clock, WallTime};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn wall(&self) -> WallTime {
        let unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        WallTime {
            unix_secs,
            utc_offset_secs: utc_offset(unix_secs),
        }
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// The local offset from UTC at `unix_secs`: `localtime_r`'s `tm_gmtoff`. 0 when the time
/// cannot be converted.
#[cfg(unix)]
fn utc_offset(unix_secs: i64) -> i32 {
    // Every target dustpan ships for has a 64-bit `time_t`.
    let t: libc::time_t = unix_secs;
    // SAFETY: a zeroed `tm` is a valid output buffer.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `t` and `tm` are valid for the call, which writes only `tm`.
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    i32::try_from(tm.tm_gmtoff).unwrap_or(0)
}

/// The current local offset from UTC: the time zone's bias, plus its daylight bias while
/// daylight time is in force. 0 when the time zone cannot be read.
#[cfg(windows)]
fn utc_offset(_: i64) -> i32 {
    /// `TIME_ZONE_ID_DAYLIGHT` from `winnt.h`.
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    use windows_sys::Win32::System::Time::{
        GetTimeZoneInformation, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    };
    // SAFETY: a zeroed `TIME_ZONE_INFORMATION` is a valid output buffer.
    let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `zone` is writable for the call.
    let which = unsafe { GetTimeZoneInformation(&mut zone) };
    if which == TIME_ZONE_ID_INVALID {
        return 0;
    }
    let daylight = if which == TIME_ZONE_ID_DAYLIGHT {
        zone.DaylightBias
    } else {
        zone.StandardBias
    };
    -(zone.Bias + daylight) * 60
}

#[cfg(test)]
mod tests {
    use super::SystemClock;
    use dustpan_core::time::Clock;

    #[test]
    fn the_offset_is_whole_minutes_within_a_day() {
        let offset = SystemClock::new().wall().utc_offset_secs;
        assert_eq!(offset % 60, 0);
        assert!(offset.abs() < 24 * 3_600);
    }
}
