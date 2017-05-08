
use bwapi_sys::bridge as sys;
use string::BwString;

pub struct Player(*mut sys::Player);

impl Player {
    pub fn get_name(&self) -> BwString {
        unsafe {
            let name = sys::Player_getName(self.0);
            BwString::from_raw(name)
        }
    }
}

