
use bwapi_sys as sys;
use std::ffi::CString;
use iterator::BwIterator;
use from_raw::FromRaw;
use unit::Unit;
use player::Player;
use region::Region;
use position::Position;
use color::Color;

use std::os::raw::c_void as void;

pub trait EventHandler {
    fn on_start(&mut self);
    fn on_end(&mut self, is_winner: bool);
    fn on_frame(&mut self);
    fn on_send_text(&mut self, text: &str);
    fn on_receive_text(&mut self, player: &mut Player, text: &str);
    fn on_player_left(&mut self, player: &mut Player);
    fn on_nuke_detect(&mut self, target: Position);
    fn on_unit_discover(&mut self, unit: &mut Unit);
    fn on_unit_evade(&mut self, unit: &mut Unit);
    fn on_unit_show(&mut self, unit: &mut Unit);
    fn on_unit_hide(&mut self, unit: &mut Unit);
    fn on_unit_create(&mut self, unit: &mut Unit);
    fn on_unit_destroy(&mut self, unit: &mut Unit);
    fn on_unit_morph(&mut self, unit: &mut Unit);
    fn on_unit_renegade(&mut self, unit: &mut Unit);
    fn on_save_game(&mut self, game_name: &str);
    fn on_unit_complete(&mut self, unit: &mut Unit);
}

pub struct Game(*mut sys::Game);

pub enum CoordinateType {
    None = 0,
    Screen = 1,
    Map = 2,
    Mouse = 3
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
    Aggressive = 4
}

impl Game {
    pub fn get() -> Game {
        unsafe {
            let game = sys::BWAPIC_getGame();
            Self::from_raw(game as *mut void)
        }
    }

    pub fn enable_flag(&self, flag: CheatFlag) {
        unsafe {
            sys::Game_enableFlag(self.0, flag as i32);
        }
    }

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

    pub fn average_fps(&self) -> f64 {
        unsafe {
            sys::Game_getAverageFPS(self.0)
        }
    }

    pub fn get_apm(&self, include_selects: bool) -> i32 {
        unsafe {
            sys::Game_getAPM(self.0, include_selects)
        }
    }

    pub fn set_command_optimization_level(&self, level: CommandOptLevel) {
        unsafe {
            sys::Game_setCommandOptimizationLevel(self.0, level as i32)
        }
    }

    pub fn draw_text(&self, ctype: CoordinateType, coords: (i32, i32), text: &str) {
        unsafe {
            let data  = CString::new(text).unwrap();
            let ctype = sys::CoordinateType { id: ctype as i32 };
            sys::Game_drawText(self.0, ctype, coords.0, coords.1, data.as_ptr());
        }
    }

    pub fn draw_line(&self, ctype: CoordinateType, first: (i32, i32), second: (i32, i32), color: Color) {
        unsafe {
            let ctype = sys::CoordinateType { id: ctype as i32 };
            sys::Game_drawLine(self.0, ctype, first.0, first.1, second.0, second.1, color.into());
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

    pub fn regions(&self) -> Box<Iterator<Item=Region>> {
        unsafe {
            let iter = sys::Game_getAllRegions(self.0) as *mut sys::Iterator;
            Box::new(BwIterator::from(iter))
        }
    }
}

impl FromRaw for Game {
    unsafe fn from_raw(raw: *mut void) -> Game {
        assert!(!raw.is_null());
        Game(raw as *mut sys::Game)
    }
}
