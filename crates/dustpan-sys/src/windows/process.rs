use dustpan_core::probe::{ProcessTable, Seen, Unseen};

/// Windows has no live-cache rule in M0, so nothing asks for this table yet. It answers
/// "unknown", which keeps any path that would need it. M8 fills it with a Toolhelp snapshot.
pub(crate) fn table() -> Seen<ProcessTable> {
    Seen::Unknown(Unseen::Unsupported)
}
