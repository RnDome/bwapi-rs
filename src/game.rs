
use bwapi_sys::bridge as sys;
use std::ffi::{CString, CStr};

pub struct Game(*mut sys::Game);

impl Game {
    pub fn send_text(&self, text: &str) {
        unsafe {
            let data = CString::new(text).unwrap();
            sys::Game_sendText(self.0, data.as_ptr());
        }
    }
}

