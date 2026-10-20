//! Tests/NoLockup.cpp: `simulate(-1)` ends, here `run(TO_THE_END)`. C++ adds
//! units with the deprecated `addUnitPlayer1/2`, which also takes units without
//! weapons; the port adds only combat units, so a side of Barracks, Dark Archon,
//! Larva or Egg stays empty.

use super::*;

fn no_lockup(first: Unit, second: Unit) {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle.add(Side::First, first).unwrap();
    battle.add(Side::Second, second).unwrap();
    battle.run(TO_THE_END);
}

#[test]
fn sunken_vs_dark_archon() {
    no_lockup(
        at(test_unit(UnitType::ZERG_SUNKEN_COLONY), 0, 0),
        at(test_unit(UnitType::PROTOSS_DARK_ARCHON), 1000, 0),
    );
}

#[test]
fn sunken_vs_corsair() {
    no_lockup(
        at(test_unit(UnitType::ZERG_SUNKEN_COLONY), 0, 0),
        at(test_unit(UnitType::PROTOSS_CORSAIR), 0, 0),
    );
}

#[test]
fn rax_mirror() {
    no_lockup(
        at(test_unit(UnitType::TERRAN_BARRACKS), 0, 0),
        at(test_unit(UnitType::TERRAN_BARRACKS), 0, 0),
    );
}

#[test]
fn scout_vs_larva() {
    no_lockup(
        at(test_unit(UnitType::PROTOSS_SCOUT), 0, 0),
        at(test_unit(UnitType::ZERG_LARVA), 0, 0),
    );
}

#[test]
fn scout_vs_egg() {
    no_lockup(
        at(test_unit(UnitType::PROTOSS_SCOUT), 0, 0),
        at(test_unit(UnitType::ZERG_EGG), 0, 0),
    );
}

#[test]
fn scout_vs_egg_with_armor_upgrades() {
    let armor_ups = Upgrades {
        armor_upgrades: 3,
        ..Upgrades::default()
    };
    no_lockup(
        at(test_unit(UnitType::PROTOSS_SCOUT), 0, 0),
        at(test_unit_with(UnitType::ZERG_EGG, &armor_ups), 0, 0),
    );
}
