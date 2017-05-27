
use bwapi_sys::bridge as sys;
use std::ffi::{CString, CStr};
use iterator::{BwIterator, FromRaw};

use unit::Unit;
use player::*;

use std::os::raw::c_void as void;

pub struct Game(*mut sys::Game);

impl FromRaw for Game {
    unsafe fn from_raw(raw: *mut void) -> Game {
        assert!(!raw.is_null());
        Game(raw as *mut sys::Game)
    }
}

pub enum CoordinateType {
    None = 0,
    Screen = 1,
    Map = 2,
    Mouse = 3
}

impl Game {
    pub fn send_text(&self, text: &str) {
        unsafe {
            let data = CString::new(text).unwrap();
            sys::Game_sendText(self.0, data.as_ptr());
        }
    }

    pub fn frame_count(&self) -> i32 {
        unsafe {
            sys::Game_getFrameCount(self.0)
        }
    }

    pub fn draw_text(&self, ctype: CoordinateType, coords: (i32, i32), text: &str) {
        unsafe {
            let data  = CString::new(text).unwrap();
            let ctype = sys::CoordinateType{ id: ctype as i32 };
            sys::Game_drawText(self.0, ctype, coords.0, coords.1, data.as_ptr());
        }
    }

    pub fn self_player(&self) -> Player {
        unsafe {
            Player::from_raw(sys::Game_self(self.0) as *mut void)
        }
    }

    pub fn minerals(&self) -> Box<Iterator<Item=Unit>> {
        unsafe {
            let iter = sys::Game_getMinerals(self.0) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }
}

