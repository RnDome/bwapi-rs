
use bwapi_sys as sys;
use std::ffi::CString;
use iterator::{BwIterator, FromRaw};

use unit::Unit;
use player::Player;
use region::Region;

use position::Position;

use std::os::raw::c_void as void;
use std::marker::PhantomData;
use std::cell::Cell;

pub trait EventHandler<'g> {
    fn on_start(&'g mut self, game: Game);
    fn on_end(&'g mut self, is_winner: bool) -> Game;
    fn on_frame(&'g mut self);
    fn on_send_text(&'g mut self, text: &str);
    fn on_receive_text(&'g mut self, player: &mut Player, text: &str);
    fn on_player_left(&'g mut self, player: &mut Player);
    fn on_nuke_detect(&'g mut self, target: Position);
    fn on_unit_discover(&'g mut self, unit: &mut Unit);
    fn on_unit_evade(&'g mut self, unit: &mut Unit);
    fn on_unit_show(&'g mut self, unit: &mut Unit);
    fn on_unit_hide(&'g mut self, unit: &mut Unit);
    fn on_unit_create(&'g mut self, unit: &mut Unit);
    fn on_unit_destroy(&'g mut self, unit: &mut Unit);
    fn on_unit_morph(&'g mut self, unit: &mut Unit);
    fn on_unit_renegade(&'g mut self, unit: &mut Unit);
    fn on_save_game(&'g mut self, game_name: &str);
    fn on_unit_complete(&'g mut self, unit: &mut Unit);
}

pub struct Game<'g> {
    raw: *mut sys::Game,
    phantom: PhantomData<Cell<&'g ()>>,
}

impl<'g> FromRaw<'g> for Game<'g> {
    unsafe fn from_raw(raw: *mut void) -> Game<'g> {
        assert!(!raw.is_null());
        Game { raw: raw as *mut sys::Game, phantom: PhantomData }
    }
}

pub enum CoordinateType {
    None = 0,
    Screen = 1,
    Map = 2,
    Mouse = 3,
}

pub enum CheatFlag {
    CompleteMapInfo = 0,
    UserInput = 1,
}

pub enum CommandOptLevel {
    None = 0,
    Some = 1,
    More = 2,
    Extensive = 3,
    Aggressive = 4,
}

impl<'g> Game<'g> {
    pub fn enable_flag(&self, flag: CheatFlag) {
        unsafe {
            sys::Game_enableFlag(self.raw, flag as i32);
        }
    }

    pub fn send_text(&self, text: &str) {
        unsafe {
            let data = CString::new(text).unwrap();
            sys::Game_sendText(self.raw, data.as_ptr());
        }
    }

    pub fn frame_count(&self) -> i32 { unsafe { sys::Game_getFrameCount(self.raw) } }

    pub fn get_apm(&self, include_selects: bool) -> i32 {
        unsafe { sys::Game_getAPM(self.raw, include_selects) }
    }

    pub fn set_command_optimization_level(&self, level: CommandOptLevel) {
        unsafe { sys::Game_setCommandOptimizationLevel(self.raw, level as i32) }
    }

    pub fn draw_text(&self, ctype: CoordinateType, coords: (i32, i32), text: &str) {
        unsafe {
            let data = CString::new(text).unwrap();
            let ctype = sys::CoordinateType { id: ctype as i32 };
            sys::Game_drawText(self.raw, ctype, coords.0, coords.1, data.as_ptr());
        }
    }

    pub fn draw_line(&self,
                     ctype: CoordinateType,
                     first: (i32, i32),
                     second: (i32, i32),
                     color: sys::Color) {
        unsafe {
            let ctype = sys::CoordinateType { id: ctype as i32 };
            sys::Game_drawLine(self.raw, ctype, first.0, first.1, second.0, second.1, color);
        }
    }

    pub fn self_player(&self) -> Player<'g> {
        unsafe { Player::from_raw(sys::Game_self(self.raw) as *mut void) }
    }

    pub fn minerals(&self) -> Box<Iterator<Item = Unit<'g>> + 'g> {
        unsafe {
            let iter = sys::Game_getMinerals(self.raw) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }

    pub fn regions(&self) -> Box<Iterator<Item = Region<'g>> + 'g> {
        unsafe {
            let iter = sys::Game_getAllRegions(self.raw) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }
}

