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

/// The Unix time of a [`local_stamp`] with its [`utc_offset`], so the inverse of the two. An
/// empty offset reads as UTC. `None` when either does not parse.
pub(crate) fn unix_secs(stamp: &str, offset: &str) -> Option<i64> {
    let digits = |bytes: &[u8]| {
        bytes.iter().try_fold(0_i64, |n, &b| {
            b.is_ascii_digit().then(|| n * 10 + i64::from(b - b'0'))
        })
    };
    let s = stamp.as_bytes();
    let shape =
        s.len() == 19 && (s[4], s[7], s[10], s[13], s[16]) == (b'-', b'-', b' ', b':', b':');
    if !shape {
        return None;
    }
    let (year, month, day) = (digits(&s[..4])?, digits(&s[5..7])?, digits(&s[8..10])?);
    let (hour, minute, second) = (digits(&s[11..13])?, digits(&s[14..16])?, digits(&s[17..])?);
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second <= 60;
    let offset_secs = match offset.as_bytes() {
        [] => 0,
        [sign @ (b'+' | b'-'), hh @ ..] if hh.len() == 4 => {
            let secs = digits(&hh[..2])? * 3_600 + digits(&hh[2..])? * 60;
            if *sign == b'-' { -secs } else { secs }
        }
        _ => return None,
    };
    valid.then(|| {
        days(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second - offset_secs
    })
}

/// Days-from-civil (H. Hinnant), the inverse of [`civil`]: days since 1970-01-01.
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let (era, yoe) = (year.div_euclid(400), year.rem_euclid(400));
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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
    use super::{WallTime, local_stamp, rfc3339_utc, unix_secs, utc_offset};

    #[test]
    fn a_stamp_and_its_offset_read_back_as_the_instant_they_print() {
        for (unix_secs_at, utc_offset_secs) in [
            (0, 0),
            (951_782_400, 0),
            (1_763_280_000, 7 * 3_600),
            (1_763_280_000, -(9 * 3_600 + 30 * 60)),
            (1_767_225_599, 14 * 3_600),
            (-86_401, -3_600),
        ] {
            let at = WallTime {
                unix_secs: unix_secs_at,
                utc_offset_secs,
            };
            assert_eq!(
                unix_secs(&local_stamp(at), &utc_offset(at)),
                Some(unix_secs_at),
                "{at:?}"
            );
        }
        assert_eq!(unix_secs("1970-01-01 01:00:00", ""), Some(3_600));
    }

    #[test]
    fn a_stamp_that_is_not_one_reads_as_none() {
        for (stamp, offset) in [
            ("2026-13-01 00:00:00", ""),
            ("2026-01-01 24:00:00", ""),
            ("2026-01-01T00:00:00", ""),
            ("2026-01-01 00:00:0x", ""),
            ("2026-01-01 00:00", ""),
            ("2026-01-01 00:00:00", "0700"),
            ("2026-01-01 00:00:00", "+07"),
            ("2026-01-01 00:00:00", "+07:0"),
            ("garbage", ""),
            ("2026-01-01 00:00:0\u{e9}", ""),
        ] {
            assert_eq!(unix_secs(stamp, offset), None, "{stamp:?} {offset:?}");
        }
    }

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
