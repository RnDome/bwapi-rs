
use bwapi_sys::bridge as sys;
use string::BwString;
use iterator::{BwIterator, FromRaw};
use std::os::raw::c_void as void;
use unit::Unit;

pub struct Player(*mut sys::Player);

impl FromRaw for Player {
    unsafe fn from_raw(raw: *mut void) -> Player {
        assert!(!raw.is_null());
        Player(raw as *mut sys::Player)
    }
}

impl Player {
    pub fn name(&self) -> BwString {
        unsafe {
            let name = sys::Player_getName(self.0);
            BwString::from_raw(name as *mut void)
        }
    }

    pub fn units(&self) -> Box<Iterator<Item=Unit>> {
        unsafe {
            let iter = sys::Player_getUnits(self.0) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }

    pub fn start_location(&self) -> sys::TilePosition {
        unsafe {
            sys::Player_getStartLocation(self.0)
        }
    }
}
