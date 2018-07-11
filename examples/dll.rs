extern crate bwapi;
extern crate bwapi_sys;

use bwapi::aimodule::wrap_handler;
use bwapi::game::{Game, CoordinateType, EventHandler};
use bwapi::unit::{Unit, UnitType};
use bwapi::player::Player;
use bwapi::position::Position;

use std::os::raw::c_void as void;

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn gameInit(game: *mut void) {
    println!("gameInit called!");

    bwapi_sys::BWAPIC_setGame(game as *mut bwapi_sys::Game);
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn newAIModule() -> *mut void {
    println!("newAIModule called!");

    let module = ExampleAIModule { name: String::from("ExampleAIModule") };
    let result = wrap_handler(Box::new(module));

    result
}

struct ExampleAIModule {
    name: String,
}

impl ExampleAIModule {
    fn draw_stat(&mut self) {
        let game = Game::get();
        let message = format!("Frame {}", game.frame_count());
        game.draw_text(CoordinateType::Screen, (10, 10), &message);
    }
    fn give_orders(&mut self) {
        let player = Game::get().self_player();

        for unit in player.units() {

            match unit.get_type() {
                UnitType::Terran_SCV |
                UnitType::Zerg_Drone |
                UnitType::Protoss_Probe => {
                    if !unit.is_idle() {
                        continue;
                    }

                    if unit.is_carrying_gas() || unit.is_carrying_minerals() {
                        unit.return_cargo(false);
                        continue;
                    }

                    if let Some(mineral) = Game::get()
                        .minerals()
                        .min_by_key(|m| unit.distance_to(m))
                    {
                        unit.right_click(&mineral, false);
                    }
                }

                UnitType::Terran_Command_Center => {
                    unit.train(UnitType::Terran_SCV);
                }

                UnitType::Protoss_Nexus => {
                    unit.train(UnitType::Protoss_Probe);
                }

                UnitType::Zerg_Hatchery |
                UnitType::Zerg_Lair |
                UnitType::Zerg_Hive => {
                    unit.train(UnitType::Zerg_Drone);
                }

                _ => {}
            };
        }
    }
}

impl EventHandler for ExampleAIModule {
    fn on_start(&mut self) {
        Game::get().send_text(&format!("Hello from Rust! My name is {}", self.name));
    }
    fn on_end(&mut self, _is_winner: bool) {}
    fn on_frame(&mut self) {
        self.draw_stat();
        self.give_orders();
    }
    fn on_send_text(&mut self, _text: &str) {}
    fn on_receive_text(&mut self, _player: &mut Player, _text: &str) {}
    fn on_player_left(&mut self, _player: &mut Player) {}
    fn on_nuke_detect(&mut self, _target: Position) {}
    fn on_unit_discover(&mut self, _unit: &mut Unit) {}
    fn on_unit_evade(&mut self, _unit: &mut Unit) {}
    fn on_unit_show(&mut self, _unit: &mut Unit) {}
    fn on_unit_hide(&mut self, _unit: &mut Unit) {}
    fn on_unit_create(&mut self, _unit: &mut Unit) {}
    fn on_unit_destroy(&mut self, _unit: &mut Unit) {}
    fn on_unit_morph(&mut self, _unit: &mut Unit) {}
    fn on_unit_renegade(&mut self, _unit: &mut Unit) {}
    fn on_save_game(&mut self, _game_name: &str) {}
    fn on_unit_complete(&mut self, _unit: &mut Unit) {}
}
