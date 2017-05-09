
use bwapi_sys::bridge as sys;
use iterator::{BwIterator, FromRaw};

pub struct Unit(*mut sys::Unit);

use std::os::raw::c_void as void;

impl FromRaw for Unit {
    unsafe fn from_raw(raw: *mut void) -> Unit {
        assert!(!raw.is_null());
        Unit(raw as *mut sys::Unit)
    }
}

impl Unit {
    pub fn exists(&self) -> bool {
        unsafe {
            sys::Unit_exists(self.0)
        }
    }

    pub fn loaded_units(&self) -> Box<Iterator<Item=Unit>> {
        unsafe {
            let iter = sys::Unit_getLoadedUnits(self.0) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }
}

