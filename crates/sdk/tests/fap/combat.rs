//! Tests/Combat.cpp. C++ adds units with the deprecated `addUnitPlayer1/2`; every
//! unit here is a combat unit, so `Battle::add` adds it too.

use super::*;

#[test]
fn marine_two_on_one() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    for _ in 0..2 {
        battle
            .add(Side::First, at(test_unit(UnitType::TERRAN_MARINE), 0, 0))
            .unwrap();
    }
    battle
        .add(Side::Second, at(test_unit(UnitType::TERRAN_MARINE), 0, 0))
        .unwrap();

    battle.run(TO_THE_END);

    // Two marines beat one.
    assert!(!battle.units(Side::First).is_empty());
    assert!(battle.units(Side::Second).is_empty());
}

#[test]
fn marine_on_zealot() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::PROTOSS_ZEALOT), 0, 0))
        .unwrap();
    battle
        .add(Side::Second, at(test_unit(UnitType::TERRAN_MARINE), 0, 0))
        .unwrap();

    battle.run(TO_THE_END);

    // Zealot beats marine.
    assert!(!battle.units(Side::First).is_empty());
    assert!(battle.units(Side::Second).is_empty());
}

#[test]
fn marine_on_zergling() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::ZERG_ZERGLING), 0, 0))
        .unwrap();
    battle
        .add(Side::Second, at(test_unit(UnitType::TERRAN_MARINE), 0, 0))
        .unwrap();

    battle.run(TO_THE_END);

    // Zergling beats marine.
    assert!(!battle.units(Side::First).is_empty());
    assert!(battle.units(Side::Second).is_empty());
}

#[test]
fn marine_firing_at_valkyrie() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::TERRAN_MARINE), 0, 0))
        .unwrap();
    battle
        .add(Side::Second, at(test_unit(UnitType::TERRAN_VALKYRIE), 0, 0))
        .unwrap();

    battle.run(1);

    // Marine does 6-2 damage to valkyrie.
    let valkyrie = &battle.units(Side::Second)[0];
    assert_eq!(valkyrie.health, valkyrie.max_health - (4 << 8));
}

#[test]
fn stacked_tank() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::PROTOSS_DRAGOON), 0, 0))
        .unwrap();
    battle
        .add(
            Side::Second,
            at(test_unit(UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE), 0, 0),
        )
        .unwrap();

    battle.run(TO_THE_END);

    assert!(!battle.units(Side::First).is_empty());
    assert!(battle.units(Side::Second).is_empty());
}

#[test]
fn non_stacked_tank() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::PROTOSS_DRAGOON), 0, 0))
        .unwrap();
    battle
        .add(
            Side::Second,
            at(test_unit(UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE), 1000, 0),
        )
        .unwrap();

    battle.run(TO_THE_END);

    assert!(battle.units(Side::First).is_empty());
    assert!(!battle.units(Side::Second).is_empty());
}

/// `testTanks<tankSplash, maxDistanceBetweenUnits>`.
fn test_tanks(tank_splash: bool, max_distance_between_units: i32) {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options { tank_splash });
    for i in 0..2 {
        let ling = at(
            test_unit(UnitType::ZERG_ZERGLING),
            1392,
            992 + max_distance_between_units * i,
        );
        battle.add(Side::First, ling).unwrap();
    }
    battle
        .add(
            Side::Second,
            at(test_unit(UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE), 992, 992),
        )
        .unwrap();

    battle.run(TO_THE_END);

    let splashed_ling_remaining_health = match (tank_splash, max_distance_between_units) {
        (true, ..=20) => 0,
        (true, ..=50) => 23,
        (true, ..=80) => 32,
        _ => 35,
    };

    // Which player wins.
    let ling_died = tank_splash && max_distance_between_units <= 20;
    let (p1, p2) = (battle.units(Side::First), battle.units(Side::Second));
    let case = format!("splash {tank_splash}, distance {max_distance_between_units}");
    assert_eq!(p1.is_empty(), ling_died, "{case}");
    assert_eq!(
        p2.is_empty(),
        !tank_splash || max_distance_between_units > 20,
        "{case}"
    );

    // Splash damage dealt, a function of the distance.
    if !ling_died {
        assert_eq!(p1[0].health >> 8, splashed_ling_remaining_health, "{case}");
    }
}

#[test]
fn one_tank_on_two_lings() {
    for distance in [20, 50, 80, 100] {
        test_tanks(true, distance);
        test_tanks(false, distance);
    }
}

/// C++ fills the vector to capacity so that the marines of a dead bunker make it
/// reallocate. Here the bunkers fill the capacity of the simulator.
#[test]
fn tank_no_crash_killing_bunker() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options { tank_splash: true });
    battle
        .add(
            Side::First,
            at(test_unit(UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE), 0, 0),
        )
        .unwrap();
    // Bunker in range of the tank, but cannot shoot back: the first unit to die
    // is a bunker.
    while battle
        .add(Side::Second, at(test_unit(UnitType::TERRAN_BUNKER), 350, 0))
        .is_ok()
    {}
    assert_eq!(battle.units(Side::Second).len(), CAPACITY / 4);

    battle.run(TO_THE_END);
}
