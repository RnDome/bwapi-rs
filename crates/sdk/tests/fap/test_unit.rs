//! `testUnit` of FAP's Tests/TestUnit.hpp: a whole unit of a type with its
//! upgrades, as `adapter::fap_unit_of_type` makes it, with 4 marines in a bunker
//! and 8 interceptors in a carrier.

use bwapi::UnitType;
use bwapi_sdk::adapter::fap_unit_of_type;
use bwapi_sdk::fap::Unit;

pub use bwapi_sdk::adapter::FapUpgrades as Upgrades;

pub fn test_unit(ut: UnitType) -> Unit {
    test_unit_with(ut, &Upgrades::default())
}

pub fn test_unit_with(ut: UnitType, upgrades: &Upgrades) -> Unit {
    let attackers = if ut == UnitType::TERRAN_BUNKER { 4 } else { 8 };
    fap_unit_of_type(ut, attackers, upgrades)
}
