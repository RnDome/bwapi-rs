
use bwapi_sys::bridge as sys;
use string::BwString;
use iterator::FromRaw;
use std::os::raw::c_void as void;

pub struct Player(*mut sys::Player);

impl FromRaw for Player {
    unsafe fn from_raw(raw: *mut void) -> Player {
        assert!(!raw.is_null());
        Player(raw as *mut sys::Player)
    }
}

impl Player {
    pub fn get_name(&self) -> BwString {
        unsafe {
            let name = sys::Player_getName(self.0);
            BwString::from_raw(name as *mut void)
        }
    }
}
