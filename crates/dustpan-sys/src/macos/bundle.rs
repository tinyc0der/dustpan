//! Which bundles an executable sits in, read from each bundle's `Info.plist`.
use core_foundation_sys::base::{CFGetTypeID, CFIndex, CFRelease, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::data::CFDataCreate;
use core_foundation_sys::dictionary::{CFDictionaryGetTypeID, CFDictionaryGetValue};
use core_foundation_sys::propertylist::{CFPropertyListCreateWithData, kCFPropertyListImmutable};
use core_foundation_sys::string::{
    CFStringCreateWithBytes, CFStringGetCString, CFStringGetLength,
    CFStringGetMaximumSizeForEncoding, CFStringGetTypeID, kCFStringEncodingUTF8,
};
use std::collections::HashMap;
use std::ffi::CStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const ID_KEY: &str = "CFBundleIdentifier";

/// What one `Info.plist` says. Kept per folder for one process snapshot: every helper of an
/// app asks about the same bundles.
#[derive(Clone)]
enum Plist {
    Id(Box<str>),
    /// No `Info.plist` here, or one that names no id: not a bundle that can own anything.
    NoId,
    Unreadable,
}

/// The `Info.plist` answers of one process snapshot, by the folder that holds the file.
#[derive(Default)]
pub(crate) struct Bundles {
    seen: HashMap<PathBuf, Plist>,
}

/// The bundle could not be read, so the process in it could be any app.
pub(crate) struct Unreadable;

impl Bundles {
    /// The id of every bundle that encloses `exe`, innermost first. A bundle keeps its
    /// `Info.plist` in `Contents` (macOS) or at its top (an iOS app on a Mac), so both places
    /// are asked on the way up.
    pub(crate) fn enclosing(&mut self, exe: &Path) -> Result<Box<[Box<str>]>, Unreadable> {
        let mut ids = Vec::new();
        for dir in exe.ancestors().skip(1) {
            let holds_plist =
                dir.file_name().is_some_and(|name| name == "Contents") || dir.extension().is_some();
            if !holds_plist {
                continue;
            }
            let plist = match self.seen.get(dir) {
                Some(plist) => plist.clone(),
                None => {
                    let plist = read(&dir.join("Info.plist"));
                    self.seen.insert(dir.to_path_buf(), plist.clone());
                    plist
                }
            };
            match plist {
                Plist::Id(id) => ids.push(id),
                Plist::NoId => {}
                Plist::Unreadable => return Err(Unreadable),
            }
        }
        Ok(ids.into())
    }
}

fn read(file: &Path) -> Plist {
    match std::fs::read(file) {
        Ok(bytes) => bundle_id(&bytes),
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => Plist::NoId,
        Err(_) => Plist::Unreadable,
    }
}

/// Releases a Core Foundation object this module created.
struct Owned(CFTypeRef);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the object came from a `Create` function, is not null, and is released once.
        unsafe { CFRelease(self.0) };
    }
}

fn owned(object: CFTypeRef) -> Option<Owned> {
    // Lazy: an `Owned` built around null would release null when it is dropped.
    (!object.is_null()).then(|| Owned(object))
}

/// `CFBundleIdentifier` from the bytes of an XML or binary property list. Core Foundation
/// parses both, as it does when the system launches the app.
fn bundle_id(bytes: &[u8]) -> Plist {
    let Ok(len) = CFIndex::try_from(bytes.len()) else {
        return Plist::Unreadable;
    };
    // SAFETY: `bytes` is readable for `len` bytes, and `CFDataCreate` copies them.
    let data = owned(unsafe { CFDataCreate(kCFAllocatorDefault, bytes.as_ptr(), len) }.cast());
    let Some(data) = data else {
        return Plist::Unreadable;
    };
    // SAFETY: `data` is a live CFData; the format and error out-pointers may be null.
    let list = owned(unsafe {
        CFPropertyListCreateWithData(
            kCFAllocatorDefault,
            data.0.cast(),
            kCFPropertyListImmutable,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    });
    let Some(list) = list else {
        return Plist::Unreadable;
    };
    // SAFETY: `list` is a live object.
    if unsafe { CFGetTypeID(list.0) != CFDictionaryGetTypeID() } {
        return Plist::Unreadable;
    }
    // SAFETY: `ID_KEY` is readable for its length and is UTF-8.
    let key = owned(
        unsafe {
            CFStringCreateWithBytes(
                kCFAllocatorDefault,
                ID_KEY.as_ptr(),
                ID_KEY.len() as CFIndex,
                kCFStringEncodingUTF8,
                0,
            )
        }
        .cast(),
    );
    let Some(key) = key else {
        return Plist::Unreadable;
    };
    // SAFETY: `list` is a dictionary and `key` a string; the value is borrowed from `list`,
    // which outlives its use below.
    let value = unsafe { CFDictionaryGetValue(list.0.cast(), key.0) };
    // SAFETY: `value` is not null when its type is read.
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return Plist::NoId;
    }
    // SAFETY: `value` is a live CFString.
    let room = unsafe {
        CFStringGetMaximumSizeForEncoding(CFStringGetLength(value.cast()), kCFStringEncodingUTF8)
    } + 1;
    let mut text = vec![0u8; usize::try_from(room).unwrap_or(0)];
    // SAFETY: `text` is writable for `room` bytes.
    let copied = unsafe {
        CFStringGetCString(
            value.cast(),
            text.as_mut_ptr().cast(),
            room,
            kCFStringEncodingUTF8,
        )
    };
    let id = (copied != 0)
        .then(|| CStr::from_bytes_until_nul(&text).ok()?.to_str().ok())
        .flatten();
    match id {
        Some(id) if !id.is_empty() => Plist::Id(id.into()),
        Some(_) => Plist::NoId,
        None => Plist::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Example</string>
<key>CFBundleIdentifier</key><string>com.example.app</string>
</dict></plist>"#;

    fn id(plist: &Plist) -> Option<&str> {
        match plist {
            Plist::Id(id) => Some(id),
            Plist::NoId | Plist::Unreadable => None,
        }
    }

    #[test]
    fn the_bundle_id_is_read_from_a_property_list() {
        assert_eq!(id(&bundle_id(XML.as_bytes())), Some("com.example.app"));
        let no_id = XML.replace("CFBundleIdentifier", "CFBundleVersion");
        assert!(matches!(bundle_id(no_id.as_bytes()), Plist::NoId));
        assert!(matches!(
            bundle_id(b"not a property list"),
            Plist::Unreadable
        ));
        assert!(matches!(
            bundle_id(b"<plist><array/></plist>"),
            Plist::Unreadable
        ));
    }

    /// `plutil -convert binary1` of a dictionary with the one key.
    const BINARY: &[u8] =
        b"bplist00\xd1\x01\x02\x5f\x10\x12CFBundleIdentifier\x5f\x10\x0fcom.example.app\
        \x08\x0b\x20\0\0\0\0\0\0\x01\x01\0\0\0\0\0\0\0\x03\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x32";

    #[test]
    fn a_binary_property_list_is_read_too() {
        assert_eq!(id(&bundle_id(BINARY)), Some("com.example.app"));
    }
}
