//! Level 2: BWAPILIB type data in a host test binary, without a game.

use bwapi::UnitType;

#[test]
fn type_data_is_available_without_a_game() {
    let marine = UnitType::TERRAN_MARINE;
    assert_eq!(marine.max_hit_points(), 40);
    assert_eq!(marine.name().to_string_lossy(), "Terran_Marine");
    assert_eq!(marine.ground_weapon().damage_amount(), 6);
    assert_eq!(marine.ground_weapon().max_range(), 128);
    assert_eq!(UnitType::TERRAN_COMMAND_CENTER.tile_width(), 4);
}
