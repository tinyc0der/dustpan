//! What the code signature of a running process says: its identifier and the app groups its
//! entitlements name. The kernel answers through `csops` for every user's processes, so no
//! subprocess runs and no file is read.
use super::plist::{Dict, Value};
use std::io;

const CS_OPS_ENTITLEMENTS_BLOB: libc::c_uint = 7;
const CS_OPS_IDENTITY: libc::c_uint = 11;
const GROUPS_KEY: &str = "com.apple.security.application-groups";

/// Every answer starts with four bytes of kind and its whole length as a big-endian `u32`.
const HEADER: usize = 8;
/// The largest answer on the bench host is 30 KB, among 808 processes.
const ROOM: usize = 64 << 10;
const MAX_ROOM: usize = 1 << 20;

unsafe extern "C" {
    /// `csops(2)`, in libSystem. The SDK declares it in `sys/codesign.h` for the kernel only.
    fn csops(
        pid: libc::pid_t,
        ops: libc::c_uint,
        useraddr: *mut libc::c_void,
        usersize: libc::size_t,
    ) -> libc::c_int;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Signature {
    pub id: Option<Box<str>>,
    pub app_groups: Box<[Box<str>]>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Unsigned {
    /// The process exited.
    Gone,
    /// The kernel would not say, or said something that cannot be read: the process could
    /// hold any group.
    Unreadable,
}

/// Asks the kernel about one process. `buf` is scratch space kept between calls.
pub(crate) fn of(pid: libc::pid_t, buf: &mut Vec<u8>) -> Result<Signature, Unsigned> {
    let id = match answer(pid, CS_OPS_IDENTITY, buf)? {
        Some(id) => {
            let id = id.split(|&b| b == 0).next().unwrap_or_default();
            let id = std::str::from_utf8(id).map_err(|_| Unsigned::Unreadable)?;
            (!id.is_empty()).then(|| id.into())
        }
        None => None,
    };
    let app_groups = match answer(pid, CS_OPS_ENTITLEMENTS_BLOB, buf)? {
        Some(entitlements) => app_groups(entitlements)?,
        None => Box::default(),
    };
    Ok(Signature { id, app_groups })
}

/// The body of one `csops` answer. `None` when the process has no such thing: it is not
/// signed, or its signature carries none.
fn answer(
    pid: libc::pid_t,
    op: libc::c_uint,
    buf: &mut Vec<u8>,
) -> Result<Option<&[u8]>, Unsigned> {
    let whole = |buf: &[u8]| u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    buf.resize(ROOM, 0);
    loop {
        buf[..HEADER].fill(0);
        // SAFETY: `buf` is writable for its whole length.
        if unsafe { csops(pid, op, buf.as_mut_ptr().cast(), buf.len()) } == 0 {
            break;
        }
        match io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => return Err(Unsigned::Gone),
            // An unsigned process, and a signature with no identifier.
            Some(libc::EINVAL | libc::ENOENT) => return Ok(None),
            // The header was written and says how much room the answer needs.
            Some(libc::ERANGE) if whole(buf) > buf.len() && whole(buf) <= MAX_ROOM => {
                buf.resize(whole(buf), 0);
            }
            _ => return Err(Unsigned::Unreadable),
        }
    }
    match whole(buf) {
        // A zeroed header: the signature has no entitlements.
        0 | HEADER => Ok(None),
        len => buf.get(HEADER..len).map(Some).ok_or(Unsigned::Unreadable),
    }
}

/// The app groups an entitlements property list names.
fn app_groups(entitlements: &[u8]) -> Result<Box<[Box<str>]>, Unsigned> {
    // Most processes name no group, and their list is not parsed at all.
    let key = GROUPS_KEY.as_bytes();
    if !entitlements.windows(key.len()).any(|at| at == key) {
        return Ok(Box::default());
    }
    let dict = Dict::parse(entitlements).map_err(|_| Unsigned::Unreadable)?;
    match dict.get(GROUPS_KEY) {
        Ok(Value::Texts(groups)) => Ok(groups),
        Ok(Value::Text(group)) => Ok(Box::new([group])),
        Ok(Value::Absent) => Ok(Box::default()),
        Ok(Value::Other) | Err(_) => Err(Unsigned::Unreadable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entitlements(value: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>com.apple.security.app-sandbox</key><true/>
<key>{GROUPS_KEY}</key>{value}
</dict></plist>"#
        )
    }

    #[test]
    fn the_app_groups_are_read_from_the_entitlements() {
        let two =
            entitlements("<array><string>group.one</string><string>TEAM.two</string></array>");
        assert_eq!(
            app_groups(two.as_bytes()),
            Ok(Box::from(["group.one".into(), "TEAM.two".into()]))
        );
        let one = entitlements("<string>group.one</string>");
        assert_eq!(
            app_groups(one.as_bytes()),
            Ok(Box::from(["group.one".into()]))
        );
        let none = entitlements("<array/>").replace(GROUPS_KEY, "com.apple.other");
        assert_eq!(app_groups(none.as_bytes()), Ok(Box::default()));
        assert_eq!(app_groups(b""), Ok(Box::default()));
    }

    #[test]
    fn entitlements_that_name_groups_and_cannot_be_read_are_unreadable() {
        let not_texts = entitlements("<true/>");
        assert_eq!(app_groups(not_texts.as_bytes()), Err(Unsigned::Unreadable));
        let whole = entitlements("<array><string>group.one</string></array>");
        let cut = &whole.as_bytes()[..whole.len() - 20];
        assert_eq!(app_groups(cut), Err(Unsigned::Unreadable));
    }

    #[test]
    fn this_process_has_a_signature_or_none_and_a_dead_pid_is_gone() {
        let mut buf = Vec::new();
        let me = libc::pid_t::try_from(std::process::id()).unwrap();
        let mine = of(me, &mut buf).expect("the kernel answers for this process");
        assert_eq!(mine.app_groups, Box::default());
        // No pid is this large: `kern.maxproc` caps them far below.
        assert_eq!(of(libc::pid_t::MAX, &mut buf), Err(Unsigned::Gone));
    }
}
