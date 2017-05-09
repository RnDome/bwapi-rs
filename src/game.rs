
use bwapi_sys::bridge as sys;
use std::ffi::{CString, CStr};

pub struct Game(*mut sys::Game);

extern "C" {
    pub fn Game_sendText(self_: *mut sys::Game, text: *const ::std::os::raw::c_char);
}

impl Game {
    pub fn send_text(&self, text: &str) {
        unsafe {
            let data = CString::new(text).unwrap();
            Game_sendText(self.0, data.as_ptr());
        }
    }
}

