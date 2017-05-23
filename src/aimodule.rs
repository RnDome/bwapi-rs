
use bwapi_sys::bridge as sys;
use unit::Unit;
use player::Player;

use std::ptr;
use std::mem;

pub trait AIHandler {
    fn on_start(&self);
    fn on_end(&mut self, isWinner: bool);
    fn on_frame(&mut self);
    fn on_send_text(&mut self, text: &str);
    fn on_receive_text(&mut self, player: &mut Player, text: &str);
    fn on_player_left(&mut self, player: &mut Player);
    // fn on_nuke_detect(&mut self, target: Position);
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

#[repr(C)]
pub struct AIModule {
    vtable: Box<sys::AIModule_vtable>,
    handler: Box<AIHandler>,
}

impl AIModule {
    pub fn new(handler: Box<AIHandler>) -> AIModule {
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

    unsafe extern fn on_start(sys_module: *mut sys::AIModule) {
        println!("on_start called!");
        let module = &* (sys_module as *mut AIModule);
        module.handler.on_start();
    }

    unsafe extern fn on_end(sys_module: *mut sys::AIModule, is_winner: bool) {}
    unsafe extern fn on_frame(sys_module: *mut sys::AIModule) {}
    unsafe extern fn on_send_text(sys_module: *mut sys::AIModule, text: *const ::std::os::raw::c_char) {}
    unsafe extern fn on_receive_text(sys_module: *mut sys::AIModule, player: *mut sys::Player, text: *const ::std::os::raw::c_char) {}
    unsafe extern fn on_player_left(sys_module: *mut sys::AIModule, player: *mut sys::Player) {}
    unsafe extern fn on_nuke_detect(sys_module: *mut sys::AIModule, target: sys::Position) {}
    unsafe extern fn on_unit_discover(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_evade(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_show(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_hide(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_create(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_destroy(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_morph(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_unit_renegade(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
    unsafe extern fn on_save_game(sys_module: *mut sys::AIModule, game_name: *const ::std::os::raw::c_char) {}
    unsafe extern fn on_unit_complete(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {}
}

pub unsafe fn wrap_handler(handler: Box<AIHandler>) -> *mut ::std::os::raw::c_void {
    let module = Box::new(AIModule::new(handler));
    let module_ptr = Box::into_raw(module) as *mut sys::AIModule;

    sys::createAIModuleWrapper(module_ptr)
}
