#![allow(unused_variables)]

extern crate bwapi;

use bwapi::aimodule::new_ai_module;
use bwapi::game::{Game, CoordinateType, EventHandler};
use bwapi::unit::{Unit, UnitId, UnitType};
use bwapi::player::Player;
use bwapi::position::Position;

use std::os::raw::c_void as void;
use std::collections::HashMap;

struct Ai<'g> {
    seen_units: HashMap<UnitId, Unit<'g>>,
}

impl<'g> Ai<'g> {
    fn draw_stat(&mut self, game: &mut Game<'g>) {
        let frame_count = game.frame_count();
        let message = format!("Frame {}", frame_count);
        game.draw_text(CoordinateType::Screen, (10, 10), &message);
    }

    fn give_orders(&mut self, game: &mut Game<'g>) {
        let player = game.self_player();

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

                    if let Some(mineral) = game
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
    fn on_start(&mut self, game: &mut Game<'g>) {
        game.send_text("Hello from Rust!");
    }

    fn on_end(&mut self, game: &mut Game<'g>, is_winner: bool) {
    }

    fn on_frame(&mut self, game: &mut Game<'g>) {
        self.draw_stat(game);
        self.give_orders(game);
    }

    fn on_send_text(&mut self, game: &mut Game<'g>, text: &str) {}
    fn on_receive_text(&mut self, game: &mut Game<'g>, player: &mut Player, text: &str) {}
    fn on_player_left(&mut self, game: &mut Game<'g>, player: &mut Player) {}
    fn on_nuke_detect(&mut self, game: &mut Game<'g>, target: Position) {}

    fn on_unit_discover(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {
        self.seen_units.insert(unit.id(), unit);
    }

    fn on_unit_evade(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_unit_show(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_unit_hide(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_unit_create(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_unit_destroy(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_unit_morph(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_unit_renegade(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
    fn on_save_game(&mut self, game: &mut Game<'g>, game_name: &str) {}
    fn on_unit_complete(&mut self, game: &mut Game<'g>, unit: Unit<'g>) {}
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn newAIModule() -> *mut void {
    println!("newAIModule called!");

    new_ai_module(|_game| Box::new(Ai { seen_units: HashMap::default() }))
}

