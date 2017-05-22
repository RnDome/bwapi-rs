
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

/// This function should be defined in the client code.
/// It should create an instance of `AIHandler` to use.
extern { fn new_handler() -> Box<AIHandler>; }

#[repr(C)]
pub struct AIModule {
    sys: sys::AIModule,
    vtable: sys::AIModule_vtable,

    handler: Box<AIHandler>,
}

impl AIModule {
    pub fn new(handler: Box<AIHandler>) -> AIModule {
        let mut module = AIModule {
            sys: sys::AIModule { vtable: ptr::null() },

            vtable: sys::AIModule_vtable {
                onStart: Some(AIModule::on_start_wrapper),
                onEnd: None,
                onFrame: None,
                onSendText: None,
                onReceiveText: None,
                onPlayerLeft: None,
                onNukeDetect: None,
                onUnitDiscover: None,
                onUnitEvade: None,
                onUnitShow: None,
                onUnitHide: None,
                onUnitCreate: None,
                onUnitDestroy: None,
                onUnitMorph: None,
                onUnitRenegade: None,
                onSaveGame: None,
                onUnitComplete: None
            },

            handler
        };

        module.sys.vtable = &module.vtable;

        module
    }

    unsafe extern fn on_start_wrapper(sys_module: *mut sys::AIModule) {
        let module = &* (sys_module as *mut AIModule);
        module.handler.on_start();
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern fn gameInit(game: *mut sys::Game) {
    print!("gameInit called!");
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern fn newAIModule() -> *mut ::std::os::raw::c_void {
    print!("newAIModule called!");

    let handler = new_handler();

    let module = Box::new(AIModule::new(handler));
    let module_ptr = Box::into_raw(module) as *mut sys::AIModule;
    let module_wrapper = sys::createAIModuleWrapper(module_ptr, 0);

    module_wrapper as *mut ::std::os::raw::c_void
}
