//! Brood War strings: bytes in Latin-1 or CP949 with color control codes, not UTF-8.

use core::fmt;
use core::ops::Deref;

/// A string returned by BWAPI.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct BwString(Vec<u8>);

impl BwString {
    pub(crate) fn from_vec(bytes: Vec<u8>) -> Self {
        BwString(bytes)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl Deref for BwString {
    type Target = BwStr;

    fn deref(&self) -> &BwStr {
        BwStr::from_bytes(&self.0)
    }
}

impl fmt::Debug for BwString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl fmt::Display for BwString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

/// A borrowed Brood War string, such as the text a callback receives.
#[derive(PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct BwStr([u8]);

impl BwStr {
    pub fn from_bytes(bytes: &[u8]) -> &BwStr {
        // SAFETY: `BwStr` is a `repr(transparent)` wrapper over `[u8]`.
        unsafe { &*(bytes as *const [u8] as *const BwStr) }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Decodes the bytes as UTF-8, replacing invalid sequences.
    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

impl fmt::Debug for BwStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_string_lossy(), f)
    }
}

impl fmt::Display for BwStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_string_lossy(), f)
    }
}

/// Writes the control byte: `format!("{}ready", TextColor::Green)`.
impl fmt::Display for crate::TextColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Every code is below 0x20: one ASCII byte, the same in UTF-8.
        let code = u8::try_from(*self as i32).expect("text codes are ASCII control bytes");
        fmt::Write::write_char(f, char::from(code))
    }
}
