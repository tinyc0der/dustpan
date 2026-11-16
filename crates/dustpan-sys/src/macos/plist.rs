//! Reads one key of a property list with Core Foundation, which parses XML and binary lists as
//! the system does when it launches an app.
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex};
use core_foundation_sys::base::{CFGetTypeID, CFIndex, CFRelease, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::data::CFDataCreate;
use core_foundation_sys::dictionary::{CFDictionaryGetTypeID, CFDictionaryGetValue};
use core_foundation_sys::propertylist::{CFPropertyListCreateWithData, kCFPropertyListImmutable};
use core_foundation_sys::string::{
    CFStringCreateWithBytes, CFStringGetCString, CFStringGetLength,
    CFStringGetMaximumSizeForEncoding, CFStringGetTypeID, kCFStringEncodingUTF8,
};
use std::ffi::CStr;

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

/// A property list whose top level is a dictionary.
pub(crate) struct Dict(Owned);

/// The bytes are not a property list with a dictionary on top, or a string in it is not text.
#[derive(Debug)]
pub(crate) struct Malformed;

/// What a dictionary holds under one key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Absent,
    Text(Box<str>),
    /// An array of strings.
    Texts(Box<[Box<str>]>),
    Other,
}

impl Dict {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let len = CFIndex::try_from(bytes.len()).map_err(|_| Malformed)?;
        // SAFETY: `bytes` is readable for `len` bytes, and `CFDataCreate` copies them.
        let data = owned(unsafe { CFDataCreate(kCFAllocatorDefault, bytes.as_ptr(), len) }.cast())
            .ok_or(Malformed)?;
        // SAFETY: `data` is a live CFData; the format and error out-pointers may be null.
        let list = owned(unsafe {
            CFPropertyListCreateWithData(
                kCFAllocatorDefault,
                data.0.cast(),
                kCFPropertyListImmutable,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        })
        .ok_or(Malformed)?;
        // SAFETY: `list` is a live object.
        if unsafe { CFGetTypeID(list.0) != CFDictionaryGetTypeID() } {
            return Err(Malformed);
        }
        Ok(Self(list))
    }

    pub(crate) fn get(&self, key: &str) -> Result<Value, Malformed> {
        // SAFETY: `key` is readable for its length and is UTF-8.
        let key = owned(
            unsafe {
                CFStringCreateWithBytes(
                    kCFAllocatorDefault,
                    key.as_ptr(),
                    key.len() as CFIndex,
                    kCFStringEncodingUTF8,
                    0,
                )
            }
            .cast(),
        )
        .ok_or(Malformed)?;
        // SAFETY: `self.0` is a dictionary and `key` a string; the value is borrowed from the
        // dictionary, which outlives its use below.
        let value = unsafe { CFDictionaryGetValue(self.0.0.cast(), key.0) };
        if value.is_null() {
            return Ok(Value::Absent);
        }
        // SAFETY: `value` is a live object.
        let kind = unsafe { CFGetTypeID(value) };
        // SAFETY: the type ids are constants of the framework.
        if kind == unsafe { CFStringGetTypeID() } {
            return text(value).map(Value::Text);
        }
        // SAFETY: as above.
        if kind != unsafe { CFArrayGetTypeID() } {
            return Ok(Value::Other);
        }
        // SAFETY: `value` is a live CFArray.
        let count = unsafe { CFArrayGetCount(value.cast()) };
        let mut texts = Vec::new();
        for at in 0..count {
            // SAFETY: `at` is within the array, and the item is borrowed from it.
            let item = unsafe { CFArrayGetValueAtIndex(value.cast(), at) };
            // SAFETY: an array item is a live object.
            if item.is_null() || unsafe { CFGetTypeID(item) != CFStringGetTypeID() } {
                return Ok(Value::Other);
            }
            texts.push(text(item)?);
        }
        Ok(Value::Texts(texts.into()))
    }
}

/// The text of a live CFString.
fn text(string: CFTypeRef) -> Result<Box<str>, Malformed> {
    // SAFETY: `string` is a live CFString.
    let room = unsafe {
        CFStringGetMaximumSizeForEncoding(CFStringGetLength(string.cast()), kCFStringEncodingUTF8)
    } + 1;
    let mut text = vec![0u8; usize::try_from(room).unwrap_or(0)];
    // SAFETY: `text` is writable for `room` bytes.
    let copied = unsafe {
        CFStringGetCString(
            string.cast(),
            text.as_mut_ptr().cast(),
            room,
            kCFStringEncodingUTF8,
        )
    };
    (copied != 0)
        .then(|| CStr::from_bytes_until_nul(&text).ok()?.to_str().ok())
        .flatten()
        .map(Into::into)
        .ok_or(Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>name</key><string>Example</string>
<key>groups</key><array><string>group.one</string><string>TEAM.two</string></array>
<key>mixed</key><array><string>group.one</string><true/></array>
<key>flag</key><true/>
</dict></plist>"#;

    #[test]
    fn a_key_holds_text_a_list_of_texts_or_something_else() {
        let dict = Dict::parse(XML.as_bytes()).unwrap();
        assert_eq!(dict.get("name").unwrap(), Value::Text("Example".into()));
        assert_eq!(
            dict.get("groups").unwrap(),
            Value::Texts(Box::new(["group.one".into(), "TEAM.two".into()]))
        );
        assert_eq!(dict.get("mixed").unwrap(), Value::Other);
        assert_eq!(dict.get("flag").unwrap(), Value::Other);
        assert_eq!(dict.get("absent").unwrap(), Value::Absent);
    }

    #[test]
    fn only_a_dictionary_parses() {
        assert!(Dict::parse(b"not a property list").is_err());
        assert!(Dict::parse(b"<plist><array/></plist>").is_err());
        assert!(Dict::parse(b"").is_err());
    }

    /// `plutil -convert binary1` of a dictionary with the one key.
    const BINARY: &[u8] =
        b"bplist00\xd1\x01\x02\x5f\x10\x12CFBundleIdentifier\x5f\x10\x0fcom.example.app\
        \x08\x0b\x20\0\0\0\0\0\0\x01\x01\0\0\0\0\0\0\0\x03\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x32";

    #[test]
    fn a_binary_property_list_parses_too() {
        let dict = Dict::parse(BINARY).unwrap();
        assert_eq!(
            dict.get("CFBundleIdentifier").unwrap(),
            Value::Text("com.example.app".into())
        );
    }
}
