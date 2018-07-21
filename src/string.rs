
use bwapi_sys as sys;
use from_raw::FromRaw;

use std::ffi::CStr;
use std::fmt;
use std::ops::Deref;
use std::os::raw::c_void as void;
use std::slice;

pub struct BwString(*mut sys::BwString);

impl BwString {
    fn as_bytes_with_nul(&self) -> &[u8] {
        unsafe {
            let raw = self.0;
            let data = sys::BwString_data(raw); // data is a pointer a nul-terminated string
            let len = sys::BwString_len(raw); // length of data without the last nul byte
            slice::from_raw_parts(data as *const u8, len as usize + 1)
        }
    }

    pub fn len(&self) -> i32 {
        unsafe {
            sys::BwString_len(self.0)
        }
    }

    pub fn data(&self) -> &CStr {
        let bytes = self.as_bytes_with_nul();
        unsafe {
            CStr::from_bytes_with_nul_unchecked(bytes)
        }
    }
}

impl Clone for BwString {
    fn clone(&self) -> BwString {
        unsafe {
            let len = self.len();
            let copy = sys::BwString_new(self.data().as_ptr(), len);
            BwString::from_raw(copy as *mut void)
        }
    }
}

impl PartialEq for BwString {
    fn eq(&self, other: &BwString) -> bool {
        self.data() == other.data()
    }
}

impl FromRaw for BwString {
    unsafe fn from_raw(raw: *mut void) -> BwString {
        let raw = raw as *mut sys::BwString;
        assert!(!raw.is_null());

        let data = sys::BwString_data(raw); // data is a pointer a nul-terminated string
        let len = sys::BwString_len(raw); // length of data without the last nul byte

        let bytes = slice::from_raw_parts(data as *const u8, len as usize + 1);
        let cstr = CStr::from_bytes_with_nul(bytes);
        assert!(cstr.is_ok());

        // TODO maybe we should check UTF-8?

        BwString(raw as *mut sys::BwString)
    }
}

impl fmt::Debug for BwString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.data().fmt(f)
    }
}

impl AsRef<CStr> for BwString {
    fn as_ref(&self) -> &CStr {
        self.data()
    }
}

impl AsRef<str> for BwString {
    fn as_ref(&self) -> &str {
        self.data().to_str().unwrap()
    }
}

impl Deref for BwString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.data().to_str().unwrap()
    }
}

impl From<BwString> for String {
    fn from(input: BwString) -> String {
        let slice: &str = input.as_ref();
        slice.to_owned()
    }
}

// impl From<St> for BwString {
//     fn from(input: BwString) -> String {
//         let slice: &str = input.as_ref();
//         slice.to_owned()
//     }
// }

impl Drop for BwString {
    fn drop(&mut self) {
        unsafe {
            sys::BwString_release(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use bwapi_sys as sys;
    use super::*;

    #[test]
    fn create_new() {
        unsafe {
            let bytes = b"any";
            let ptr = bytes.as_ptr() as *const i8;
            let len = bytes.len() as i32;

            let sys_string = sys::BwString_new(ptr, len);
            BwString::from_raw(sys_string as *mut void);
        }
    }

    #[test]
    fn conversions() {
        let input = "Hello world!";

        let string = unsafe {
            let ptr = input.as_ptr() as *const i8;
            let len = input.len() as i32;

            let sys_string = sys::BwString_new(ptr, len);
            BwString::from_raw(sys_string as *mut void)
        };

        assert_eq!(input.len(), string.len() as usize);
        assert_eq!(input, string.data().to_str().unwrap());
        assert_eq!(input, <BwString as AsRef<str>>::as_ref(&string));
        assert_eq!(input, String::from(string.as_ref()));
        assert_eq!(input, String::from(string));

        // let test = |input: &str| println!("input is {}", input);
        // test(&string);
    }
}
