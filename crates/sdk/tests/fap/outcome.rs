//! What `run` tells of the frames it ran and why it stopped.

use bwapi_sdk::fap::{Battle, Outcome};

use super::*;

/// A zealot and a valkyrie, which cannot hit each other.
fn standoff(simulator: &mut Simulator) -> Battle<'_> {
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::PROTOSS_ZEALOT), 0, 0))
        .unwrap();
    battle
        .add(Side::Second, at(test_unit(UnitType::TERRAN_VALKYRIE), 0, 0))
        .unwrap();
    battle
}

#[test]
fn a_side_without_units_runs_no_frame() {
    let mut simulator = simulator();
    let mut battle = simulator.battle(Options::default());
    battle
        .add(Side::First, at(test_unit(UnitType::TERRAN_MARINE), 0, 0))
        .unwrap();
    let outcome = battle.run(10);
    assert_eq!(outcome, Outcome::SideEliminated { frames: 0 });
}

#[test]
fn no_frames_asked_runs_none() {
    let outcome = standoff(&mut simulator()).run(0);
    assert_eq!(outcome, Outcome::LimitReached { frames: 0 });
}

#[test]
fn an_idle_frame_stops_and_counts() {
    let outcome = standoff(&mut simulator()).run(10);
    assert_eq!(outcome, Outcome::Idle { frames: 1 });
}

#[test]
fn the_limit_comes_before_idleness() {
    let outcome = standoff(&mut simulator()).run(1);
    assert_eq!(outcome, Outcome::LimitReached { frames: 1 });
}

#[test]
fn a_fight_to_the_end_eliminates_a_side() {
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
    let outcome = battle.run(TO_THE_END);
    assert!(battle.units(Side::Second).is_empty());
    assert!(matches!(outcome, Outcome::SideEliminated { frames } if frames > 0));
}
