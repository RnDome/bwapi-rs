//! The Rust port of FAP against FAP in C++, on the random fights of the tests
//! of `bwapi-sdk` (`crates/sdk/tests/fight`), simulated step by step with and
//! without tank splash. Every state must be the same.

#![cfg(fap_reference)]

#[path = "../../sdk/tests/fight/mod.rs"]
mod fight;
#[path = "../../sdk/tests/property/mod.rs"]
mod property;

use std::time::Instant;

use bwapi::UnitType;
use fight::{FRAMES_PER_STEP, Fighter, LONG_RUN, STEPS};
use openbw_host::fap_reference::{self, Spec};
use property::fap::{self as checks, FRAMES, Fight, Kind, LONG_FIGHTS, Made};
use property::mechanics::{self, Body};
use property::{Case, Offenders, Side};

/// The fights, each from its own seed.
const FIGHTS: u64 = 20_000;

/// A body of the duels and controlled fights of `bwapi-sdk`'s
/// `tests/property/mechanics`.
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

fn spec(f: &Fighter) -> Spec {
    Spec {
        player: f.player,
        type_id: f.unit_type.id(),
        x: f.x,
        y: f.y,
        health: f.health,
        shields: f.shields,
        armor_upgrades: f.armor_upgrades,
        attack_upgrades: f.attack_upgrades,
        shield_upgrades: f.shield_upgrades,
        flags: f.flags,
        elevation: f.elevation,
    }
}

#[test]
fn the_port_matches_fap() {
    let mut differences = Vec::new();
    let (mut fought, mut unfinished) = (0, 0);
    // Seconds of C++ and of the port: adding the units, simulating, reading
    // the state after each step.
    let (mut cpp, mut rust) = (0.0, 0.0);
    for seed in 0..FIGHTS {
        let (fighters, tank_splash) = fight::fight(seed);
        let specs: Vec<Spec> = fighters.iter().map(spec).collect();
        for (steps, frames) in [(STEPS, FRAMES_PER_STEP), (1, LONG_RUN)] {
            let lap = Instant::now();
            let theirs = fap_reference::simulate(&specs, tank_splash, steps, frames);
            cpp += lap.elapsed().as_secs_f64();
            let lap = Instant::now();
            let ours = fight::simulate(&fighters, tank_splash, steps, frames);
            rust += lap.elapsed().as_secs_f64();
            if let Some(step) =
                (0..theirs.len().max(ours.len())).find(|&i| theirs.get(i) != ours.get(i))
            {
                differences.push(format!(
                    "fight {seed} ({steps} x {frames} frames, splash {tank_splash}), step {step}:\n  port {:?}\n  C++  {:?}",
                    ours.get(step),
                    theirs.get(step)
                ));
            }
            // A long run where both sides lost units; one where both stand.
            if frames == LONG_RUN {
                let end = theirs.last().expect("a state");
                let lost = |p: usize| end[p].len() < theirs[0][p].len();
                fought += usize::from(lost(0) && lost(1));
                unfinished += usize::from(!end[0].is_empty() && !end[1].is_empty());
            }
        }
    }
    println!(
        "{FIGHTS} fights, {fought} costing both sides units, {unfinished} with both sides left \
         after the long run: C++ {cpp:.3} s, port {rust:.3} s"
    );
    assert!(
        differences.is_empty(),
        "{} fights differ:\n{}",
        differences.len(),
        differences[..differences.len().min(3)].join("\n")
    );
    // A comparison of no fighting passes.
    assert!(
        fought as u64 > FIGHTS / 10,
        "only {fought} fights cost both sides units"
    );
}

/// A fight observed frame by frame for `limit` frames by `run(mode, frames)`,
/// C++ or the port, as `bwapi-sdk`'s `fap_properties` observes the port.
fn observe(
    name: &str,
    id: i32,
    fighters: &[Fighter],
    made: Made,
    limit: i32,
    run: impl Fn(i32, i32) -> Vec<Vec<i32>>,
) -> Fight {
    let kinds = fighters
        .iter()
        .map(|f| Kind::of(f.player, f.unit_type))
        .collect();
    checks::observe(
        name.to_owned(),
        id,
        kinds,
        UnitType::TERRAN_MARINE.max_hit_points() << 8,
        made,
        limit,
        run,
    )
}

/// The properties of `bwapi-sdk`'s `tests/property/fap` on FAP in C++, and
/// the port alike after every frame.
#[test]
fn cpp_fap_has_the_properties() {
    let mut fights: Vec<(String, Vec<Fighter>, bool, Made, i32)> = (0..FIGHTS)
        .map(|seed| {
            let (fighters, splash) = fight::fight(seed);
            let limit = if seed < LONG_FIGHTS {
                checks::LONG_RUN
            } else {
                FRAMES
            };
            (
                format!("fight {seed}"),
                fighters,
                splash,
                Made::Random,
                limit,
            )
        })
        .collect();
    for (name, fighters, splash, no_attacks) in fight::controlled() {
        let made = if no_attacks {
            Made::ControlledWithoutAttacks
        } else {
            Made::Controlled
        };
        fights.push((name.to_owned(), fighters, splash, made, FRAMES));
    }

    let mut observed = Vec::new();
    let mut parity = Offenders::new("fap_frame_parity", "fights");
    for (i, (name, fighters, splash, made, limit)) in fights.iter().enumerate() {
        let id = if i < FIGHTS as usize {
            i as i32
        } else {
            1_000_000 + (i - FIGHTS as usize) as i32
        };
        let specs: Vec<Spec> = fighters.iter().map(spec).collect();
        let cpp = observe(name, id, fighters, *made, *limit, |mode, frames| {
            fap_reference::run(&specs, *splash, mode, frames)
        });
        let port = observe(name, id, fighters, *made, *limit, |mode, frames| {
            fight::run(fighters, *splash, mode, frames)
        });
        if let Some(difference) = checks::parity(&port, &cpp) {
            parity.add_keyed((id, 0, 0, 0), || difference);
        }
        observed.push(cpp);
    }

    let mut violations = checks::check(&observed);
    let run = |bodies: &[Body], splash: bool, mode: i32, frames: i32| {
        let specs: Vec<Spec> = bodies.iter().map(|b| spec(&fighter(b))).collect();
        fap_reference::run(&specs, splash, mode, frames)
    };
    violations.extend(mechanics::check(&observed, fight::TYPES, &run));
    parity.report(&mut |property, subject, detail| {
        violations.push(property::Violation {
            property: property.to_owned(),
            subject,
            detail,
        })
    });
    let case = Case {
        side: Side::Cpp,
        map: "fap".to_owned(),
        race: String::new(),
        state: "fights".to_owned(),
    };
    let checked = vec![(case, violations)];
    for line in property::diagnostics(&checked) {
        println!("{line}");
    }
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/tests/data");
    let failures = property::judge(&checked, &property::known_file(&data));
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
