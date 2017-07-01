
use bwapi_sys as sys;
use unit::Unit;
use player::Player;
use iterator::FromRaw;
use game;

use std::ffi::CStr;
use std::os::raw::c_void as void;

static mut GAME: *mut void = 0 as *mut void;

type HandlerFactory = Box<'static + for<'g> FnMut(&mut game::Game<'g>) -> Box<game::EventHandler<'g> + 'g>>;

#[repr(C)]
pub struct AIModule {
    vtable: Box<sys::AIModule_vtable>,
    handler_factory: HandlerFactory,
    handler: Option<*mut game::EventHandler<'static>>,
}

impl AIModule{
    pub fn new(factory: HandlerFactory) -> AIModule {
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

            handler_factory: factory,
            handler: None
        }
    }

    unsafe fn get_handler<'a>(sys_module: *mut sys::AIModule) -> &'a mut game::EventHandler<'static> {
        let module = sys_module as *mut AIModule;
        &mut *(*module).handler.unwrap()
    }

    unsafe extern "C" fn on_start(sys_module: *mut sys::AIModule) {
        let game = game::Game::from_raw(GAME);
        Self::get_handler(sys_module).on_start(game);
    }

    unsafe extern "C" fn on_end(sys_module: *mut sys::AIModule, is_winner: bool) {
        Self::get_handler(sys_module).on_end(is_winner);
    }

    unsafe extern "C" fn on_frame(sys_module: *mut sys::AIModule) {
        Self::get_handler(sys_module).on_frame();
    }

    unsafe extern "C" fn on_send_text(
        sys_module: *mut sys::AIModule,
        text: *const ::std::os::raw::c_char,
    ) {
        let text = CStr::from_ptr(text)
            .to_str()
            .unwrap();
        Self::get_handler(sys_module).on_send_text(&text);
    }

    unsafe extern "C" fn on_receive_text(
        sys_module: *mut sys::AIModule,
        player: *mut sys::Player,
        text: *const ::std::os::raw::c_char,
    ) {
        let mut player = Player::from_raw(player as *mut void);
        let text = CStr::from_ptr(text)
            .to_str()
            .unwrap();
        Self::get_handler(sys_module).on_receive_text(&mut player, &text);
    }

    unsafe extern "C" fn on_player_left(sys_module: *mut sys::AIModule, player: *mut sys::Player) {
        let mut player = Player::from_raw(player as *mut void);
        Self::get_handler(sys_module).on_player_left(&mut player);
    }

    unsafe extern "C" fn on_nuke_detect(sys_module: *mut sys::AIModule, target: sys::Position) {
        Self::get_handler(sys_module).on_nuke_detect(target.into());
    }

    unsafe extern "C" fn on_unit_discover(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_discover(&mut unit);
    }

    unsafe extern "C" fn on_unit_evade(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_evade(&mut unit);
    }

    unsafe extern "C" fn on_unit_show(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_show(&mut unit);
    }

    unsafe extern "C" fn on_unit_hide(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_hide(&mut unit);
    }

    unsafe extern "C" fn on_unit_create(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_create(&mut unit);
    }

    unsafe extern "C" fn on_unit_destroy(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_destroy(&mut unit);
    }

    unsafe extern "C" fn on_unit_morph(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_morph(&mut unit);
    }

    unsafe extern "C" fn on_unit_renegade(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_renegade(&mut unit);
    }

    unsafe extern "C" fn on_save_game(
        sys_module: *mut sys::AIModule,
        game_name: *const ::std::os::raw::c_char,
    ) {
        let game_name = CStr::from_ptr(game_name)
            .to_str()
            .unwrap();
        Self::get_handler(sys_module).on_save_game(game_name);
    }

    unsafe extern "C" fn on_unit_complete(sys_module: *mut sys::AIModule, unit: *mut sys::Unit) {
        let mut unit = Unit::from_raw(unit as *mut void);
        Self::get_handler(sys_module).on_unit_complete(&mut unit);
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn gameInit(game: *mut void) {
    println!("gameInit called!");
    GAME = game;
}

pub fn new_ai_module<F>(factory: F) -> *mut ::std::os::raw::c_void
    where F: 'static + for<'g> FnMut(&mut game::Game<'g>) -> Box<game::EventHandler<'g> + 'g>
{
    let module = Box::new(AIModule::new(Box::new(factory)));
    let module_ptr = Box::into_raw(module) as *mut sys::AIModule;

    unsafe {
        sys::createAIModuleWrapper(module_ptr)
    }
}
