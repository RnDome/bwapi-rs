
use bwapi_sys as sys;
use string::BwString;
use iterator::{BwIterator, FromRaw};
use std::os::raw::c_void as void;
use unit::Unit;
use game::Game;

use std::marker::PhantomData;

#[derive(Clone)]
pub struct Player<'g> {
    raw: *mut sys::Player,
    phantom: PhantomData<&'g Game>,
}

impl<'g> FromRaw for Player<'g> {
    unsafe fn from_raw(raw: *mut void) -> Player<'g> {
        assert!(!raw.is_null());
        Player { raw: raw as *mut sys::Player, phantom: PhantomData }
    }
}

impl<'g> Player<'g> {
    pub fn name(&self) -> BwString {
        unsafe {
            let name = sys::Player_getName(self.raw);
            BwString::from_raw(name as *mut void)
        }
    }

    pub fn units(&self) -> Box<Iterator<Item = Unit<'g>>> {
        unsafe {
            let iter = sys::Player_getUnits(self.raw) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }

    pub fn start_location(&self) -> sys::TilePosition {
        unsafe { sys::Player_getStartLocation(self.raw) }
    }
}

