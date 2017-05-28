
use bwapi_sys::bridge as sys;
use iterator::{BwIterator, FromRaw};

pub struct Region(*mut sys::Region);

use std::os::raw::c_void as void;


impl FromRaw for Region {
    unsafe fn from_raw(raw: *mut void) -> Region {
        assert!(!raw.is_null());
        Region(raw as *mut sys::Region)
    }
}

impl Region {
    pub fn id(&self) -> i32 {
        unsafe {
            sys::Region_getID(self.0)
        }
    }

    pub fn group_id(&self) -> i32 {
        unsafe {
            sys::Region_getRegionGroupID(self.0)
        }
    }

    pub fn center(&self) -> sys::Position {
        unsafe {
            sys::Region_getCenter(self.0)
        }
    }

    pub fn defense_priority(&self) -> i32 {
        unsafe {
            sys::Region_getDefensePriority(self.0)
        }
    }

    pub fn neighbors(&self) -> Box<Iterator<Item=Region>> {
        unsafe {
            let iter = sys::Region_getNeighbors(self.0) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }
}
