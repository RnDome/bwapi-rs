//! Random fights for FAP, the same on every run: the units of `testUnit` of
//! FAP's tests with random types, positions, health, shields and upgrades.
//! `openbw-host`'s `fap_parity` runs them against FAP in C++.

#![allow(dead_code)]

use bwapi::UnitType;
use bwapi_sdk::adapter::{FapUpgrades, fap_unit_of_type};
use bwapi_sdk::fap::{self, Battle, FapUnit, Options, Outcome, Side, Simulator};

/// Steps of each fight, then one long run.
pub const STEPS: i32 = 6;
pub const FRAMES_PER_STEP: i32 = 24;

/// The long run: five minutes of game time. Not until the fight is over: some
/// fights never are, and `simulate(-1)` then never returns in C++, nor does
/// `run(u32::MAX)` in the port in any time worth waiting.
pub const LONG_RUN: i32 = 24 * 60 * 5;

/// What fights: the units FAP simulates, the static defense among them.
pub const TYPES: &[UnitType] = &[
    UnitType::TERRAN_SCV,
    UnitType::TERRAN_MARINE,
    UnitType::TERRAN_FIREBAT,
    UnitType::TERRAN_MEDIC,
    UnitType::TERRAN_GHOST,
    UnitType::TERRAN_VULTURE,
    UnitType::TERRAN_VULTURE_SPIDER_MINE,
    UnitType::TERRAN_SIEGE_TANK_TANK_MODE,
    UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE,
    UnitType::TERRAN_GOLIATH,
    UnitType::TERRAN_WRAITH,
    UnitType::TERRAN_VALKYRIE,
    UnitType::TERRAN_BATTLECRUISER,
    UnitType::TERRAN_MISSILE_TURRET,
    UnitType::TERRAN_BUNKER,
    UnitType::ZERG_DRONE,
    UnitType::ZERG_ZERGLING,
    UnitType::ZERG_HYDRALISK,
    UnitType::ZERG_LURKER,
    UnitType::ZERG_MUTALISK,
    UnitType::ZERG_GUARDIAN,
    UnitType::ZERG_DEVOURER,
    UnitType::ZERG_SCOURGE,
    UnitType::ZERG_ULTRALISK,
    UnitType::ZERG_INFESTED_TERRAN,
    UnitType::ZERG_SUNKEN_COLONY,
    UnitType::ZERG_SPORE_COLONY,
    UnitType::PROTOSS_PROBE,
    UnitType::PROTOSS_ZEALOT,
    UnitType::PROTOSS_DRAGOON,
    UnitType::PROTOSS_DARK_TEMPLAR,
    UnitType::PROTOSS_ARCHON,
    UnitType::PROTOSS_REAVER,
    UnitType::PROTOSS_SCARAB,
    UnitType::PROTOSS_SCOUT,
    UnitType::PROTOSS_CORSAIR,
    UnitType::PROTOSS_CARRIER,
    UnitType::PROTOSS_INTERCEPTOR,
    UnitType::PROTOSS_ARBITER,
    UnitType::PROTOSS_PHOTON_CANNON,
];

/// A unit of a fight: `testUnit` at a position, with its health, shields and
/// upgrades.
#[derive(Clone, Copy, Debug)]
pub struct Fighter {
    /// 1 or 2.
    pub player: i32,
    pub unit_type: UnitType,
    pub x: i32,
    pub y: i32,
    pub health: i32,
    pub shields: i32,
    pub armor_upgrades: i32,
    pub attack_upgrades: i32,
    pub shield_upgrades: i32,
    /// 1 speed, 2 attack speed, 4 range upgrade, 8 stimmed.
    pub flags: i32,
    /// Ground height, -1 for unknown, as `testUnit` leaves it.
    pub elevation: i32,
}

/// What is compared of a `FAPUnit`, in this order: x, y, health, max health,
/// armor, shields, max shields, shield armor, the bits of speed and of its
/// square, flying, elevation; ground damage, cooldown, squared max and min
/// range and damage type id; the same for air; unit size id, organic, healed
/// this frame, attackers, attack cooldown remaining.
pub type Unit = [i32; 27];

/// Both players' units after a step.
pub type State = [Vec<Unit>; 2];

/// xorshift64: the same fights on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// In `lo..=hi`.
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
}

/// The units of fight `seed`, and whether tanks splash: up to ten a side, a
/// side's upgrades shared, the sides `64..=512` pixels apart; half the units
/// hurt.
pub fn fight(seed: u64) -> (Vec<Fighter>, bool) {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    let distance = rng.range(64, 512);
    let mut fighters = Vec::new();
    for player in 1..=2 {
        let (armor, attack, shield, flags) = (
            rng.range(0, 3),
            rng.range(0, 3),
            rng.range(0, 3),
            rng.range(0, 15),
        );
        for _ in 0..rng.range(1, 10) {
            let ut = TYPES[rng.range(0, TYPES.len() as i32 - 1) as usize];
            let hurt = rng.range(0, 1) == 1;
            let health = if hurt {
                rng.range(1, ut.max_hit_points().max(1))
            } else {
                ut.max_hit_points()
            };
            let shields = if hurt {
                rng.range(0, ut.max_shields())
            } else {
                ut.max_shields()
            };
            fighters.push(Fighter {
                player,
                unit_type: ut,
                x: 1000 + if player == 1 { 0 } else { distance } + rng.range(-96, 96),
                y: 1000 + rng.range(-96, 96),
                health,
                shields,
                armor_upgrades: armor,
                attack_upgrades: attack,
                shield_upgrades: shield,
                flags,
                elevation: -1,
            });
        }
    }
    (fighters, rng.range(0, 1) == 1)
}

/// The fields [`Unit`] lists.
fn fields<T>(u: &FapUnit<T>) -> Unit {
    [
        u.x,
        u.y,
        u.health,
        u.max_health,
        u.armor,
        u.shields,
        u.max_shields,
        u.shield_armor,
        u.speed.to_bits() as i32,
        u.speed_squared.to_bits() as i32,
        i32::from(u.flying),
        u.elevation,
        u.ground_damage,
        u.ground_cooldown,
        u.ground_max_range_squared,
        u.ground_min_range_squared,
        u.ground_damage_type as i32,
        u.air_damage,
        u.air_cooldown,
        u.air_max_range_squared,
        u.air_min_range_squared,
        u.air_damage_type as i32,
        u.unit_size as i32,
        i32::from(u.is_organic),
        i32::from(u.did_heal_this_frame),
        u.attacker_count,
        u.attack_cooldown_remaining,
    ]
}

/// The fighter as FAP takes it, and its side.
fn unit<T: Default>(f: &Fighter) -> (Side, fap::Unit<T>) {
    let upgrades = FapUpgrades {
        armor_upgrades: f.armor_upgrades,
        attack_upgrades: f.attack_upgrades,
        shield_upgrades: f.shield_upgrades,
        speed_upgrade: f.flags & 1 != 0,
        attack_speed_upgrade: f.flags & 2 != 0,
        range_upgrade: f.flags & 4 != 0,
        stimmed: f.flags & 8 != 0,
    };
    let attackers = if f.unit_type == UnitType::TERRAN_BUNKER {
        4
    } else {
        8
    };
    let mut unit = fap_unit_of_type(f.unit_type, attackers, &upgrades);
    (unit.x, unit.y, unit.health, unit.shields) = (f.x, f.y, f.health, f.shields);
    unit.elevation = f.elevation;
    let side = if f.player == 1 {
        Side::First
    } else {
        Side::Second
    };
    (side, unit)
}

/// The port on a fight for the properties of FAP, as `openbw_host_fap_run`
/// (`openbw-host`'s `cpp/fap_reference.cpp`) runs C++: each unit tagged with
/// its index among `fighters`, the messages of the run `mode` of `frames`.
pub fn run(fighters: &[Fighter], tank_splash: bool, mode: i32, frames: i32) -> Vec<Vec<i32>> {
    let options = Options { tank_splash };
    let count = u32::try_from(frames).expect("a count of frames, not negative");
    let mut simulator: Simulator<i32> = Simulator::with_capacity(64);
    let add = |battle: &mut Battle<'_, i32>| {
        for (i, f) in fighters.iter().enumerate() {
            let (side, mut unit) = unit::<i32>(f);
            unit.data = i as i32;
            battle.add(side, unit).expect("room for the fight");
        }
    };
    let message = |battle: &Battle<'_, i32>, outcome: Option<Outcome>| {
        let (reason, ran) = match outcome {
            None => (-1, -1),
            Some(Outcome::LimitReached { frames }) => (0, frames as i32),
            Some(Outcome::SideEliminated { frames }) => (1, frames as i32),
            Some(Outcome::Idle { frames }) => (2, frames as i32),
        };
        let mut out = vec![reason, ran];
        for side in [Side::First, Side::Second] {
            let units = battle.units(side);
            out.push(units.len() as i32);
            for u in units {
                out.extend(fields(u));
                out.push(u.data);
            }
        }
        out
    };
    if mode == 1 {
        let mut battle = simulator.battle(options);
        add(&mut battle);
        battle.run(count);
    }
    let mut battle = simulator.battle(options);
    add(&mut battle);
    let mut messages = Vec::new();
    match mode {
        0 | 1 => {
            messages.push(message(&battle, None));
            for _ in 0..frames {
                let outcome = battle.run(1);
                messages.push(message(&battle, Some(outcome)));
                if outcome.frames() == 0 {
                    break;
                }
            }
        }
        2 => {
            let outcome = battle.run(count);
            messages.push(message(&battle, Some(outcome)));
        }
        _ => {
            for _ in 1..frames {
                battle.run(1);
            }
            let outcome = battle.run(2);
            messages.push(message(&battle, Some(outcome)));
        }
    }
    messages
}

/// A fighter of a controlled fight, without upgrades, at `x`, 1000.
fn plain(player: i32, unit_type: UnitType, x: i32, health: i32, shields: i32) -> Fighter {
    Fighter {
        player,
        unit_type,
        x,
        y: 1000,
        health,
        shields,
        armor_upgrades: 0,
        attack_upgrades: 0,
        shield_upgrades: 0,
        flags: 0,
        elevation: -1,
    }
}

/// Fights made to show one thing, with tank splash or not, and whether no unit
/// can hit any enemy:
/// - a wounded medic alone, against a spore colony that hits only air: the
///   medic heals, nothing else happens;
/// - wounded zerg and protoss on both sides, none of which can hit an enemy:
///   regeneration only;
/// - a bunker of player one, about to die to a zealot: its marines are made in
///   player two's turn and do nothing before the frame ends;
/// - the same of player two.
pub fn controlled() -> Vec<(&'static str, Vec<Fighter>, bool, bool)> {
    let t = UnitType::TERRAN_MEDIC;
    vec![
        (
            "a medic alone",
            vec![
                plain(1, t, 1000, 20, 0),
                plain(2, UnitType::ZERG_SPORE_COLONY, 1300, 400, 0),
            ],
            false,
            true,
        ),
        (
            "regeneration only",
            vec![
                plain(1, UnitType::ZERG_ZERGLING, 1000, 10, 0),
                plain(1, UnitType::PROTOSS_ZEALOT, 1000, 100, 0),
                plain(2, UnitType::TERRAN_VALKYRIE, 1100, 100, 0),
                plain(2, UnitType::PROTOSS_CORSAIR, 1100, 50, 0),
                plain(2, UnitType::ZERG_SCOURGE, 1100, 10, 0),
            ],
            false,
            true,
        ),
        (
            "a bunker of player one dies",
            vec![
                plain(1, UnitType::TERRAN_BUNKER, 1000, 1, 0),
                plain(2, UnitType::PROTOSS_ZEALOT, 1000, 100, 60),
            ],
            false,
            false,
        ),
        (
            "a bunker of player two dies",
            vec![
                plain(1, UnitType::PROTOSS_ZEALOT, 1000, 100, 60),
                plain(2, UnitType::TERRAN_BUNKER, 1000, 1, 0),
            ],
            false,
            false,
        ),
    ]
}

/// The port on a fight: the state before the first step and after each of
/// `steps` steps of `frames` frames.
pub fn simulate(fighters: &[Fighter], tank_splash: bool, steps: i32, frames: i32) -> Vec<State> {
    let count = u32::try_from(frames).expect("a count of frames, not negative");
    let mut simulator = Simulator::with_capacity(64);
    let mut battle = simulator.battle(Options { tank_splash });
    for f in fighters {
        let (side, unit) = unit(f);
        battle.add(side, unit).expect("room for the fight");
    }
    let state = |battle: &Battle| {
        [
            battle.units(Side::First).iter().map(fields).collect(),
            battle.units(Side::Second).iter().map(fields).collect(),
        ]
    };
    let mut states = vec![state(&battle)];
    for _ in 0..steps {
        battle.run(count);
        states.push(state(&battle));
    }
    states
}
