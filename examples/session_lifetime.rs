extern crate bwapi;
extern crate bwapi_sys;

use bwapi::aimodule::wrap_handler;
use bwapi::game::{self, Game, CoordinateType, EventHandler};
use bwapi::unit::{Unit, UnitId, UnitType};
use bwapi::player::Player;

use std::os::raw::c_void as void;
use std::collections::HashMap;

struct Context<'g> {
    seen_units: HashMap<UnitId, Unit<'g>>,
}

type Session<'g> = game::Session<Context<'g>>;

struct Ai<'g> {
    context: Option<Session<'g>>
}

impl<'g> Ai<'g> {
    fn session(&mut self) -> &mut Session<'g> {
        self.context.as_mut().unwrap()
    }

    fn game(&mut self) -> &Game {
        self.context.as_mut().unwrap().game()
    }

    fn draw_stat(&mut self) {
        let game = self.game();
        let message = format!("Frame {}", game.frame_count());
        game.draw_text(CoordinateType::Screen, (10, 10), &message);
    }

    fn give_orders(&mut self) {
        let player = self.session().game().self_player();

        #[cfg_attr(rustfmt, rustfmt_skip)]
        for unit in player.units() {

            for unit in unit.loaded_units() {
                let data = self.session().data_mut();
                data.seen_units.insert(unit.id(), unit);
            }

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

                    if let Some(mineral) = self.session().game()
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

impl<'g> EventHandler<'g> for Ai<'g> {
    fn on_start(&'g mut self, game: Game) {
        self.context = Some(Session::new(game, Context { seen_units: HashMap::new() }));
        //self.game = Some(game);
        // self.init();
        let build = if cfg!(debug_assertions) { "debug" } else { "release" };

        self.session().game().send_text(&format!("Hello from Rust {}!", build));
    }

    fn on_end(&'g mut self, is_winner: bool) -> Game {
        self.context.take().unwrap().release()
    }

    fn on_frame(&'g mut self) {
        self.draw_stat();
        self.give_orders();
        // let session = self.session();
        // Ai::give_orders(session);
    }

    fn on_send_text(&'g mut self, text: &str) {}
    fn on_receive_text(&'g mut self, player: &mut Player, text: &str) {}
    fn on_player_left(&'g mut self, player: &mut Player) {}
    // fn on_nuke_detect(&mut self, target: Position) {}
    fn on_unit_discover(&'g mut self, unit: &mut Unit) {

    }
    fn on_unit_evade(&'g mut self, unit: &mut Unit) {}
    fn on_unit_show(&'g mut self, unit: &mut Unit) {}
    fn on_unit_hide(&'g mut self, unit: &mut Unit) {}
    fn on_unit_create(&'g mut self, unit: &mut Unit) {}
    fn on_unit_destroy(&'g mut self, unit: &mut Unit) {}
    fn on_unit_morph(&'g mut self, unit: &mut Unit) {}
    fn on_unit_renegade(&'g mut self, unit: &mut Unit) {}
    fn on_save_game(&'g mut self, game_name: &str) {}
    fn on_unit_complete(&'g mut self, unit: &mut Unit) {}
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn newAIModule() -> *mut void {
    println!("newAIModule called!");

    let handler = Ai { context: None };
    let result = wrap_handler(Box::new(handler));

    result
}


