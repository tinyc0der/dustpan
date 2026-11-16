//! The protection policy. Per-OS tables are data, one file per OS and the only list of what that
//! OS protects. `compiled` turns a table into the matcher a `Pass` walks. Everything here is
//! private to core: callers see only verdicts.
//!
//! Every table compiles on every host, so a typo in the Windows table fails `cargo check` on a
//! Mac. A table is selected by an `Os` value, not by `cfg`, so the macOS rules run in unit tests
//! on the Linux CI runner too (both use `/` paths); Windows rules run on the Windows runner.
mod compiled;
mod linux;
mod live;
mod macos;
mod macos_bundles;
mod names;
mod table;
mod windows;

pub(crate) use compiled::{Policy, Step};
pub(crate) use live::{LiveChecks, ScopeOwner};

use crate::paths::Os;
use table::Table;

fn table(os: Os) -> &'static Table {
    match os {
        Os::MacOs => &macos::TABLE,
        Os::Linux => &linux::TABLE,
        Os::Windows => &windows::TABLE,
    }
}
