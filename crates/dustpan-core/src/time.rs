//! Time as data. Core never reads a clock: the binary and the privileged helper pass
//! `dustpan_sys::SystemClock`, and tests pass [`FixedClock`].
use std::time::Duration;

/// Wall-clock time with the local UTC offset, for the stamps of log rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WallTime {
    pub unix_secs: i64,
    pub utc_offset_secs: i32,
}

pub trait Clock: Sync {
    fn wall(&self) -> WallTime;
    /// Monotonic time since a fixed start. The guard ages shared live evidence with it
    /// (`guard::Lease`), so a wall-clock jump never extends that evidence.
    fn elapsed(&self) -> Duration;
}

/// `YYYY-MM-DD HH:MM:SS` in local time: the stamp of Mole's row grammar and session markers.
pub fn local_stamp(at: WallTime) -> String {
    let local = at.unix_secs.saturating_add(i64::from(at.utc_offset_secs));
    let (year, month, day, secs) = civil(local);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

/// `+HHMM` or `-HHMM`, as `date +%z` prints it.
pub fn utc_offset(at: WallTime) -> String {
    let sign = if at.utc_offset_secs < 0 { '-' } else { '+' };
    let minutes = at.utc_offset_secs.unsigned_abs() / 60;
    format!("{sign}{:02}{:02}", minutes / 60, minutes % 60)
}

/// Civil-from-days (H. Hinnant), shifted so the year starts on 1 March: year, month, day and
/// seconds into the day.
fn civil(unix_secs: i64) -> (i64, i64, i64, i64) {
    let (days, rem) = (unix_secs.div_euclid(86_400), unix_secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rem)
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339_utc(unix_secs: u64) -> String {
    let (year, month, day, rem) = civil(i64::try_from(unix_secs).unwrap_or(i64::MAX));
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// A clock a test sets and advances by hand.
#[cfg(any(test, feature = "testkit"))]
pub struct FixedClock {
    wall: WallTime,
    elapsed: std::sync::Mutex<Duration>,
}

#[cfg(any(test, feature = "testkit"))]
impl FixedClock {
    pub fn new(wall: WallTime) -> Self {
        Self {
            wall,
            elapsed: std::sync::Mutex::new(Duration::ZERO),
        }
    }

    pub fn advance(&self, by: Duration) {
        *self.elapsed.lock().unwrap_or_else(|e| e.into_inner()) += by;
    }
}

#[cfg(any(test, feature = "testkit"))]
impl Clock for FixedClock {
    fn wall(&self) -> WallTime {
        self.wall
    }

    fn elapsed(&self) -> Duration {
        *self.elapsed.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::{WallTime, local_stamp, rfc3339_utc, utc_offset};

    #[test]
    fn known_instants() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_731_679_457), "2024-11-15T14:04:17Z");
    }

    #[test]
    fn local_stamps_carry_the_offset_separately() {
        let at = |utc_offset_secs| WallTime {
            unix_secs: 1_763_280_000,
            utc_offset_secs,
        };
        assert_eq!(local_stamp(at(0)), "2025-11-16 08:00:00");
        assert_eq!(local_stamp(at(7 * 3_600)), "2025-11-16 15:00:00");
        assert_eq!(utc_offset(at(7 * 3_600)), "+0700");
        assert_eq!(utc_offset(at(-(9 * 3_600 + 30 * 60))), "-0930");
        assert_eq!(utc_offset(at(0)), "+0000");
    }
}
