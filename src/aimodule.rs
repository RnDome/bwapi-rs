
use bwapi_sys::bridge as sys;
use unit::Unit;
use player::Player;
use iterator::FromRaw;
use game;

use std::ptr;
use std::mem;
use std::ffi::CStr;
use std::os::raw::c_void as void;

#[repr(C)]
pub struct AIModule {
    vtable: Box<sys::AIModule_vtable>,
    handler: Box<game::EventHandler>,
}

impl AIModule {
    pub fn new(handler: Box<game::EventHandler>) -> AIModule {
        AIModule {
            vtable: Box::new(sys::AIModule_vtable {
                onStart: Some(AIModule::on_start),
                onEnd: Some(AIModule::on_end),
                onFrame: Some(AIModule::on_frame),
                onSendText: Some(AIModule::on_send_text),
                onReceiveText: Some(AIModule::on_receive_text),
                onPlayerLeft: Some(AIModule::on_player_left),
                onNukeDetect: Some(AIModule::on_nuke_detect),
                onUnitDiscover: Some(AIModule::on_unit_discover),
                onUnitEvade: Some(AIModule::on_unit_evade),
                onUnitShow: Some(AIModule::on_unit_show),
                onUnitHide: Some(AIModule::on_unit_hide),
                onUnitCreate: Some(AIModule::on_unit_create),
                onUnitDestroy: Some(AIModule::on_unit_destroy),
                onUnitMorph: Some(AIModule::on_unit_morph),
                onUnitRenegade: Some(AIModule::on_unit_renegade),
                onSaveGame: Some(AIModule::on_save_game),
                onUnitComplete: Some(AIModule::on_unit_complete),
            }),

            handler
        }
    }

    unsafe fn get_handler<'a>(sys_module: *mut sys::AIModule) -> &'a mut game::EventHandler {
        let module = sys_module as *mut AIModule;
        &mut * (*module).handler
    }

    unsafe extern fn on_start(sys_module: *mut sys::AIModule) {
        Self::get_handler(sys_module).on_start();
    }

    unsafe extern fn on_end(sys_module: *mut sys::AIModule, is_winner: bool) {
        Self::get_handler(sys_module).on_end(is_winner);
    }

    unsafe extern fn on_frame(sys_module: *mut sys::AIModule) {
        Self::get_handler(sys_module).on_frame();
    }

    unsafe extern fn on_send_text(sys_module: *mut sys::AIModule, text: *const ::std::os::raw::c_char) {
        let text = CStr::from_ptr(text).to_str().unwrap();
        Self::get_handler(sys_module).on_send_text(&text);
    }

    unsafe extern fn on_receive_text(sys_module: *mut sys::AIModule, player: *mut sys::Player, text: *const ::std::os::raw::c_char) {
        let mut player = Player::from_raw(player as *mut void);
        let text = CStr::from_ptr(text).to_str().unwrap();
        Self::get_handler(sys_module).on_receive_text(&mut player, &text);
    }

    unsafe extern fn on_player_left(sys_module: *mut sys::AIModule, player: *mut sys::Player) {
        let mut player = Player::from_raw(player as *mut void);
        Self::get_handler(sys_module).on_player_left(&mut player);
    }

    unsafe extern fn on_nuke_detect(sys_module: *mut sys::AIModule, target: sys::Position) {
        // TODO
    }

    unsafe extern fn on_unit_discover(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_discover(&mut unit);
    }

    unsafe extern fn on_unit_evade(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_evade(&mut unit);
    }

    unsafe extern fn on_unit_show(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_show(&mut unit);
    }

    unsafe extern fn on_unit_hide(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_hide(&mut unit);
    }

    unsafe extern fn on_unit_create(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_create(&mut unit);
    }

    unsafe extern fn on_unit_destroy(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_destroy(&mut unit);
    }

    unsafe extern fn on_unit_morph(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_morph(&mut unit);
    }

    unsafe extern fn on_unit_renegade(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_renegade(&mut unit);
    }

    unsafe extern fn on_save_game(sys_module: *mut sys::AIModule, game_name: *const ::std::os::raw::c_char) {
        let game_name = CStr::from_ptr(game_name).to_str().unwrap();
        Self::get_handler(sys_module).on_save_game(game_name);
    }

    unsafe extern fn on_unit_complete(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_complete(&mut unit);
    }

}

pub unsafe fn wrap_handler(handler: Box<game::EventHandler>) -> *mut ::std::os::raw::c_void {
    let module = Box::new(AIModule::new(handler));
    let module_ptr = Box::into_raw(module) as *mut sys::AIModule;

    sys::createAIModuleWrapper(module_ptr)
}
