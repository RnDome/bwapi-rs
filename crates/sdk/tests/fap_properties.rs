//! The properties of `tests/property/fap` and `tests/property/mechanics` on
//! the port of FAP: the random fights of `tests/fight` and its controlled
//! ones, the duels and controlled fights of the mechanics, as `openbw-host`'s
//! `fap_parity` runs them on C++ and checks the same.

mod answer;
mod fight;
mod property;

use bwapi::UnitType;
use fight::Fighter;
use property::fap::{self, FRAMES, Fight, Kind, LONG_FIGHTS, LONG_RUN, Made};
use property::mechanics::{self, Body};
use property::{Case, Side};

/// The fights, each from its own seed: those of `fap_answers`.
const FIGHTS: u64 = 20_000;

/// A body of the duels and controlled fights of `tests/property/mechanics`.
fn fighter(b: &Body) -> Fighter {
    Fighter {
        player: b.player,
        unit_type: b.unit_type,
        x: b.x,
        y: b.y,
        health: b.health,
        shields: b.shields,
        armor_upgrades: b.armor_upgrades,
        attack_upgrades: b.attack_upgrades,
        shield_upgrades: b.shield_upgrades,
        flags: 0,
        elevation: b.elevation,
    }
}

/// The port on a fight of `fighters`, frame by frame for `limit` frames.
fn observe(
    name: String,
    id: i32,
    fighters: &[Fighter],
    splash: bool,
    made: Made,
    limit: i32,
) -> Fight {
    let kinds = fighters
        .iter()
        .map(|f| Kind::of(f.player, f.unit_type))
        .collect();
    fap::observe(
        name,
        id,
        kinds,
        UnitType::TERRAN_MARINE.max_hit_points() << 8,
        made,
        limit,
        |mode, frames| fight::run(fighters, splash, mode, frames),
    )
}

#[test]
fn fap_results_have_their_properties() {
    let seeds: Vec<u64> = (0..FIGHTS).collect();
    let mut fights = answer::parallel(&seeds, |&seed| {
        let (fighters, splash) = fight::fight(seed);
        let limit = if seed < LONG_FIGHTS { LONG_RUN } else { FRAMES };
        observe(
            format!("fight {seed}"),
            seed as i32,
            &fighters,
            splash,
            Made::Random,
            limit,
        )
    });
    for (i, (name, fighters, splash, no_attacks)) in fight::controlled().into_iter().enumerate() {
        let made = if no_attacks {
            Made::ControlledWithoutAttacks
        } else {
            Made::Controlled
        };
        fights.push(observe(
            name.to_owned(),
            1_000_000 + i as i32,
            &fighters,
            splash,
            made,
            FRAMES,
        ));
    }
    let case = Case {
        side: Side::Port,
        map: "fap".to_owned(),
        race: String::new(),
        state: "fights".to_owned(),
    };
    let mut violations = fap::check(&fights);
    let run = |bodies: &[Body], splash: bool, mode: i32, frames: i32| {
        let fighters: Vec<Fighter> = bodies.iter().map(fighter).collect();
        fight::run(&fighters, splash, mode, frames)
    };
    violations.extend(mechanics::check(&fights, fight::TYPES, &run));
    let checked = vec![(case, violations)];
    for line in property::diagnostics(&checked) {
        println!("{line}");
    }
    let failures = property::judge(&checked, &property::known_file(&answer::data()));
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
