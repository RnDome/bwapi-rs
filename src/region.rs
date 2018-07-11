
use bwapi_sys as sys;
use iterator::{BwIterator, FromRaw};
use position::Position;

use std::os::raw::c_void as void;

use std::marker::PhantomData;
use std::cell::Cell;

#[derive(Clone)]
pub struct Region<'g> {
    raw: *mut sys::Region,
    phantom: PhantomData<Cell<&'g ()>>,
}

impl<'g> FromRaw<'g> for Region<'g> {
    unsafe fn from_raw(raw: *mut void) -> Region<'g> {
        assert!(!raw.is_null());
        Region { raw: raw as *mut sys::Region, phantom: PhantomData }
    }
}

impl<'g> Region<'g> {
    pub fn id(&self) -> i32 {
        unsafe {
            sys::Region_getID(self.raw)
        }
    }

    pub fn group_id(&self) -> i32 {
        unsafe {
            sys::Region_getRegionGroupID(self.raw)
        }
    }

    pub fn center(&self) -> Position {
        Position::from( unsafe { sys::Region_getCenter(self.raw) } )
    }

    pub fn defense_priority(&self) -> i32 {
        unsafe {
            sys::Region_getDefensePriority(self.raw)
        }
    }

    pub fn neighbors(&self) -> Box<Iterator<Item=Region<'g>> + 'g> {
        unsafe {
            let iter = sys::Region_getNeighbors(self.raw) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }
}
