use dustpan_core::probe::{OpenFiles, ProcessTable, Seen, Unseen};

/// Windows has no live-cache rule in M0, so nothing asks for this table yet. It answers
/// "unknown", which keeps any path that would need it. M8 fills it with a Toolhelp snapshot.
pub(crate) fn table() -> Seen<ProcessTable> {
    Seen::Unknown(Unseen::Unsupported)
}

pub(crate) mod open {
    use super::{OpenFiles, Seen, Unseen};

    /// Listing another process's handles takes `NtQuerySystemInformation`, which M8 adds.
    /// Until then the answer is "unknown": a path no rule protects is kept, and `explain`
    /// says this build cannot check it.
    pub(crate) fn open_files() -> Seen<OpenFiles> {
        Seen::Unknown(Unseen::Unsupported)
    }
}
