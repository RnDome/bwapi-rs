
use bwapi_sys::bridge as sys;

use std::ffi::CStr;
use std::fmt;
use std::ops::Deref;

pub struct BwString(*mut sys::String);

impl BwString {
    pub unsafe fn from_raw(raw: *mut sys::String) -> BwString {
        // TODO Perform checks here and maintain invariant later
        BwString(raw)
    }

    pub fn len(&self) -> usize {
        unsafe {
            sys::String_len(self.0)
        }
    }

    pub fn data(&self) -> &CStr {
        unsafe {
            let data = sys::String_data(self.0);

            // TODO from_bytes_with_nul_unchecked()
            CStr::from_ptr(data)
        }
    }
}

impl Clone for BwString {
    fn clone(&self) -> BwString {
        unsafe {
            BwString::from_raw(self.0)
        }
    }
}

impl PartialEq for BwString {
    fn eq(&self, other: &BwString) -> bool {
        self.data() == other.data()
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

impl Drop for BwString {
    fn drop(&mut self) {
        unsafe {
            sys::String_release(self.0);
        }
    }
}

#[test]
fn conversions() {
    let input = "Hello world!";

    let string = unsafe {
        let bytes: Vec<i8> = input.bytes().chain(Some(0)).map(|x| x as i8).collect();
        let sys_string = sys::String_new(bytes.as_ptr(), input.len());
        BwString::from_raw(sys_string)
    };

    assert_eq!(input.len(), string.len() - 1);
    assert_eq!(input, string.data().to_str().unwrap());
    assert_eq!(input, <BwString as AsRef<str>>::as_ref(&string));
    assert_eq!(input, String::from(string.as_ref()));
    assert_eq!(input, String::from(string));

    // let test = |input: &str| println!("input is {}", input);
    // test(&string);
}
