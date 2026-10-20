//! Level 2: BWAPILIB type data in a host test binary, without a game.

use bwapi::UnitType;
use bwapi_sdk::adapter::footprint;
use bwapi_sdk::bweb::Footprint;

#[test]
fn the_types_of_the_bweb_presets_are_those_of_bwapi() {
    let types = (0..234).map(UnitType::from_id);
    let known: Vec<UnitType> = types.filter(|&t| Footprint::of(t).is_some()).collect();
    assert_eq!(known.len(), 16);
    let wrong: Vec<(Option<Footprint>, Footprint)> = known
        .into_iter()
        .map(|t| (Footprint::of(t), footprint(t)))
        .filter(|(ours, theirs)| *ours != Some(*theirs))
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn type_data_is_available_without_a_game() {
    let marine = UnitType::TERRAN_MARINE;
    assert_eq!(marine.max_hit_points(), 40);
    assert_eq!(marine.name().to_string_lossy(), "Terran_Marine");
    assert_eq!(marine.ground_weapon().damage_amount(), 6);
    assert_eq!(marine.ground_weapon().max_range(), 128);
    assert_eq!(UnitType::TERRAN_COMMAND_CENTER.tile_width(), 4);
}
