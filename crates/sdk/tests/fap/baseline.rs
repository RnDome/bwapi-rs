//! Reference results of the FAP port before any change to its mechanics.
//!
//! Each scenario is simulated in steps and the state of every unit is recorded
//! after each step. A change of a result must be intended: then replace the
//! table with the one the failing test prints and say why in the commit.

use std::fmt::Write;

use super::*;

/// Frames of each step; `-1` runs to the end of the fight, `TO_THE_END`.
const STEPS: [i32; 3] = [24, 48, -1];

struct Scenario {
    name: &'static str,
    one: Vec<Unit>,
    two: Vec<Unit>,
    options: Options,
}

/// `count` units of a type in a row along `y`, `gap` pixels apart.
fn row(ut: UnitType, count: i32, x: i32, y: i32, gap: i32) -> Vec<Unit> {
    (0..count)
        .map(|i| at(test_unit(ut), x, y + i * gap))
        .collect()
}

fn elevated(mut unit: Unit, elevation: i32) -> Unit {
    unit.elevation = elevation;
    unit
}

fn scenarios() -> Vec<Scenario> {
    let plain = Options::default();
    let splash = Options { tank_splash: true };
    let stim = Upgrades {
        stimmed: true,
        ..Upgrades::default()
    };
    vec![
        Scenario {
            name: "marine vs marine",
            one: row(UnitType::TERRAN_MARINE, 1, 0, 0, 0),
            two: row(UnitType::TERRAN_MARINE, 1, 200, 0, 0),
            options: plain,
        },
        Scenario {
            name: "zealot vs marines",
            one: row(UnitType::PROTOSS_ZEALOT, 1, 0, 0, 0),
            two: row(UnitType::TERRAN_MARINE, 2, 300, 0, 16),
            options: plain,
        },
        Scenario {
            name: "marines and medic vs marines",
            one: [
                row(UnitType::TERRAN_MARINE, 3, 0, 0, 16),
                row(UnitType::TERRAN_MEDIC, 1, -32, 16, 0),
            ]
            .concat(),
            two: row(UnitType::TERRAN_MARINE, 4, 160, 0, 16),
            options: plain,
        },
        Scenario {
            name: "stimmed marines vs zerglings",
            one: (0..4)
                .map(|i| at(test_unit_with(UnitType::TERRAN_MARINE, &stim), 0, i * 16))
                .collect(),
            two: row(UnitType::ZERG_ZERGLING, 8, 400, 0, 12),
            options: plain,
        },
        Scenario {
            name: "siege tank splash vs zerglings",
            one: row(UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE, 1, 0, 0, 0),
            two: row(UnitType::ZERG_ZERGLING, 6, 420, 0, 10),
            options: splash,
        },
        Scenario {
            name: "bunker vs zealots",
            one: row(UnitType::TERRAN_BUNKER, 1, 0, 0, 0),
            two: row(UnitType::PROTOSS_ZEALOT, 3, 300, 0, 20),
            options: plain,
        },
        Scenario {
            name: "dragoon uphill vs dragoon",
            one: vec![elevated(at(test_unit(UnitType::PROTOSS_DRAGOON), 0, 0), 0)],
            two: vec![elevated(
                at(test_unit(UnitType::PROTOSS_DRAGOON), 300, 0),
                2,
            )],
            options: plain,
        },
        Scenario {
            name: "hydralisks vs mutalisks",
            one: row(UnitType::ZERG_HYDRALISK, 4, 0, 0, 20),
            two: row(UnitType::ZERG_MUTALISK, 4, 300, 0, 20),
            options: plain,
        },
        Scenario {
            name: "scourge vs mutalisks",
            one: row(UnitType::ZERG_SCOURGE, 4, 0, 0, 10),
            two: row(UnitType::ZERG_MUTALISK, 2, 300, 0, 20),
            options: plain,
        },
        Scenario {
            name: "zealots vs zerglings",
            one: row(UnitType::PROTOSS_ZEALOT, 2, 0, 0, 20),
            two: row(UnitType::ZERG_ZERGLING, 6, 200, 0, 12),
            options: plain,
        },
        Scenario {
            name: "no contact",
            one: row(UnitType::TERRAN_MARINE, 1, 0, 0, 0),
            two: row(UnitType::TERRAN_MARINE, 1, 3000, 0, 0),
            options: plain,
        },
    ]
}

/// The units of each side after each step: `x,y hp/shields` in 1/256.
fn run(scenario: Scenario) -> String {
    let mut simulator = simulator();
    let mut battle = simulator.battle(scenario.options);
    for unit in scenario.one {
        battle.add(Side::First, unit).unwrap();
    }
    for unit in scenario.two {
        battle.add(Side::Second, unit).unwrap();
    }
    let mut out = String::new();
    for frames in STEPS {
        battle.run(u32::try_from(frames).unwrap_or(TO_THE_END));
        let (one, two) = (battle.units(Side::First), battle.units(Side::Second));
        for (side, units) in [("1", one), ("2", two)] {
            write!(out, "{frames}:{side}[").unwrap();
            for u in units {
                write!(out, " {},{} {}/{}", u.x, u.y, u.health, u.shields).unwrap();
            }
            out.push_str(" ] ");
        }
    }
    out.truncate(out.trim_end().len());
    out
}

const BASELINE: &[(&str, &str)] = &[
    (
        "marine vs marine",
        "24:1[ 39,0 8704/0 ] 24:2[ 165,0 8704/0 ] 48:1[ 39,0 2560/0 ] 48:2[ 165,0 2560/0 ] -1:1[ ] -1:2[ 165,0 1024/0 ]",
    ),
    (
        "zealot vs marines",
        "24:1[ 95,0 25600/13838 ] 24:2[ 213,0 10240/0 228,16 10240/0 ] 48:1[ 198,0 25600/3422 ] 48:2[ 213,0 6144/0 225,16 10240/0 ] -1:1[ 213,9 21341/70 ] -1:2[ ]",
    ),
    (
        "marines and medic vs marines",
        "24:1[ 16,0 8368/0 16,16 8704/0 16,32 4096/0 16,0 15360/0 ] 24:2[ 144,0 7168/0 144,16 7168/0 144,32 8704/0 142,48 8704/0 ] 48:1[ 16,0 7168/0 16,16 3238/0 16,16 15360/0 ] 48:2[ 144,0 2560/0 144,16 4096/0 141,32 8704/0 139,48 4096/0 ] -1:1[ ] -1:2[ 133,46 1024/0 141,16 4096/0 138,32 5632/0 ]",
    ),
    (
        "stimmed marines vs zerglings",
        "24:1[ 142,0 10240/0 120,16 10240/0 120,32 10240/0 142,48 10240/0 ] 24:2[ 280,0 8960/0 280,12 8960/0 280,24 8960/0 280,36 8960/0 280,48 8960/0 280,60 8960/0 280,72 8960/0 280,79 8960/0 ] 48:1[ 140,32 1280/0 140,16 10240/0 ] 48:2[ 148,40 7460/0 150,42 8960/0 160,3 7572/0 152,40 1456/0 150,42 8960/0 ] -1:1[ ] -1:2[ 144,25 5984/0 145,27 8960/0 152,8 6096/0 146,28 1516/0 145,27 8960/0 ]",
    ),
    (
        "siege tank splash vs zerglings",
        "24:1[ 0,0 38400/0 ] 24:2[ 300,30 4544/0 300,50 4544/0 300,40 4544/0 ] 48:1[ 0,0 38400/0 ] 48:2[ 60,12 4736/0 60,12 4736/0 60,12 4736/0 ] -1:1[ ] -1:2[ 10,2 5164/0 10,2 5164/0 10,2 5164/0 ]",
    ),
    (
        "bunker vs zealots",
        "24:1[ 0,0 89600/0 ] 24:2[ 204,0 25600/15360 228,20 25600/15360 228,40 25600/15360 ] 48:1[ 0,0 89600/0 ] 48:2[ 15,0 23271/21 84,20 25600/15360 84,22 25600/15360 ] -1:1[ 0,0 1792/0 ] -1:2[ ]",
    ),
    (
        "dragoon uphill vs dragoon",
        "24:1[ 90,0 25600/15409 ] 24:2[ 215,0 25600/15402 ] 48:1[ 90,0 25600/10625 ] 48:2[ 215,0 25600/15738 ] -1:1[ ] -1:2[ 215,0 22416/210 ]",
    ),
    (
        "hydralisks vs mutalisks",
        "24:1[ 60,0 20480/0 60,20 20480/0 60,40 20480/0 60,60 20480/0 ] 24:2[ 156,0 29456/0 156,20 29456/0 156,40 29456/0 156,60 29456/0 ] 48:1[ 60,0 16064/0 60,20 16064/0 60,40 16064/0 60,60 16064/0 ] 48:2[ 156,0 25808/0 156,20 25808/0 156,40 25808/0 156,60 25808/0 ] -1:1[ ] -1:2[ 156,0 7500/0 156,20 7500/0 156,40 7500/0 156,60 7500/0 ]",
    ),
    (
        "scourge vs mutalisks",
        "24:1[ 144,0 4124/0 144,9 6400/0 144,20 4124/0 144,29 6400/0 ] 24:2[ 198,0 30720/0 198,20 30720/0 ] 48:1[ ] 48:2[ ] -1:1[ ] -1:2[ ]",
    ),
    (
        "zealots vs zerglings",
        "24:1[ 84,0 25600/10261 74,16 25600/15360 ] 24:2[ 95,0 6924/0 95,5 8960/0 93,6 8960/0 95,8 8960/0 92,8 8960/0 96,12 8960/0 ] 48:1[ 80,14 25600/12807 ] 48:2[ 95,0 7116/0 95,5 8960/0 89,8 928/0 90,10 8960/0 93,9 8960/0 ] -1:1[ ] -1:2[ 88,6 7368/0 91,7 8960/0 89,8 1180/0 93,9 8960/0 ]",
    ),
    (
        "no contact",
        "24:1[ 92,0 10240/0 ] 24:2[ 2904,0 10240/0 ] 48:1[ 280,0 10240/0 ] 48:2[ 2716,0 10240/0 ] -1:1[ 1437,0 1024/0 ] -1:2[ ]",
    ),
];

#[test]
fn results_match_the_baseline() {
    let actual: Vec<(&str, String)> = scenarios().into_iter().map(|s| (s.name, run(s))).collect();
    let matches = actual.len() == BASELINE.len()
        && actual
            .iter()
            .zip(BASELINE)
            .all(|((name, got), (bname, want))| name == bname && got == want);
    if !matches {
        let mut table = String::from("const BASELINE: &[(&str, &str)] = &[\n");
        for (name, got) in &actual {
            writeln!(table, "    ({name:?}, {got:?}),").unwrap();
        }
        table.push_str("];");
        panic!("FAP results differ from the baseline; the current table:\n{table}");
    }
}
