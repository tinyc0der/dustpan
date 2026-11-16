//! Allocated size (SAFE-21). Unknown never renders or sums as 0.
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Known(u64),
    /// A sizing deadline or a permission gap cut the walk short. Renders as `N+`.
    Partial(u64),
    Unknown,
}

impl Size {
    /// The sum of two sizes. Anything short of two known sizes is a partial total: an unknown
    /// part adds no bytes and marks the total `N+`, never `0` (SAFE-21).
    pub fn plus(self, other: Size) -> Size {
        match (self, other) {
            (Self::Known(a), Self::Known(b)) => Self::Known(a.saturating_add(b)),
            (Self::Known(a) | Self::Partial(a), Self::Partial(b))
            | (Self::Partial(a), Self::Known(b)) => Self::Partial(a.saturating_add(b)),
            (Self::Known(a) | Self::Partial(a), Self::Unknown)
            | (Self::Unknown, Self::Known(a) | Self::Partial(a)) => Self::Partial(a),
            (Self::Unknown, Self::Unknown) => Self::Unknown,
        }
    }
}

/// SI units, as Finder and `diskutil` print them: `999 B`, `1.5 MB`.
fn si(f: &mut fmt::Formatter<'_>, bytes: u64) -> fmt::Result {
    const UNITS: [&str; 6] = ["kB", "MB", "GB", "TB", "PB", "EB"];
    if bytes < 1000 {
        return write!(f, "{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    // 999,950 bytes would print as `1000.0 kB`; it steps up to `1.0 MB` instead.
    while value >= 999.95 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    write!(f, "{value:.1} {}", UNITS[unit])
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Known(bytes) => si(f, bytes),
            Self::Partial(bytes) => {
                si(f, bytes)?;
                f.write_str("+")
            }
            Self::Unknown => f.write_str("unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Size;

    #[test]
    fn sizes_print_in_si_units() {
        assert_eq!(Size::Known(0).to_string(), "0 B");
        assert_eq!(Size::Known(999).to_string(), "999 B");
        assert_eq!(Size::Known(1000).to_string(), "1.0 kB");
        assert_eq!(Size::Known(1_500_000).to_string(), "1.5 MB");
        assert_eq!(Size::Known(999_950).to_string(), "1.0 MB");
        assert_eq!(Size::Known(u64::MAX).to_string(), "18.4 EB");
    }

    #[test]
    fn a_partial_size_is_marked_and_an_unknown_size_is_never_zero() {
        assert_eq!(Size::Partial(12_300_000_000).to_string(), "12.3 GB+");
        assert_eq!(Size::Unknown.to_string(), "unknown");
        assert_ne!(Size::Unknown, Size::Known(0));
    }

    #[test]
    fn a_sum_with_an_unknown_part_is_partial_never_zero() {
        assert_eq!(Size::Known(1).plus(Size::Known(2)), Size::Known(3));
        assert_eq!(Size::Known(1).plus(Size::Unknown), Size::Partial(1));
        assert_eq!(Size::Unknown.plus(Size::Partial(4)), Size::Partial(4));
        assert_eq!(Size::Unknown.plus(Size::Unknown), Size::Unknown);
    }
}
