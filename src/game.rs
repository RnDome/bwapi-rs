
use bwapi_sys::bridge as sys;
use std::ffi::{CString, CStr};
use iterator::FromRaw;
use std::os::raw::c_void as void;

pub struct Game(*mut sys::Game);

impl FromRaw for Game {
    unsafe fn from_raw(raw: *mut void) -> Game {
        assert!(!raw.is_null());
        Game(raw as *mut sys::Game)
    }
}

impl Game {
    pub fn send_text(&self, text: &str) {
        unsafe {
            let data = CString::new(text).unwrap();
            sys::Game_sendText(self.0, data.as_ptr());
        }
    }
}

