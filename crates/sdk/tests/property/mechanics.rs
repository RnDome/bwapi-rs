//! Properties of the mechanics of FAP: damage, targets, reach, splash,
//! suicide units, attacking uphill. The state of a fight is in
//! [`super::fap`]; these use its records and its runs.
//!
//! Duels set one attacker of player one against one target of player two and
//! check every hit against the damage of FAP, written anew over the fields of
//! the units, and against the damage of Brood War as OpenBW deals it
//! (`weapon_deal_damage` of `bwgame.h`). Controlled fights show one mechanic
//! each. The random fights of [`super::fap`] check reach and steps.
//!
//! The damage of FAP (`dealDamage`), in 1/256: the damage, shifted; shields
//! take it less their armor, and if they hold, that is all; else what they
//! did not take goes on, less the armor of the unit, scaled by damage type and
//! size, at least 128 off the hit points.

use std::collections::BTreeMap;

use bwapi::UnitType;
use bwapi_sdk::adapter::{FapUpgrades, fap_unit_of_type};

use super::Violation;
use super::fap::{
    AIR_DAMAGE, AIR_DAMAGE_TYPE, AIR_MAX_RANGE, ARMOR, ATTACKERS, COOLDOWN, FIELDS, FLYING,
    FRAME_BY_FRAME, Fight, GROUND_DAMAGE, GROUND_DAMAGE_TYPE, GROUND_MAX_RANGE, HEALTH, Kind,
    MAX_HEALTH, MAX_SHIELDS, Message, Report, SHIELD_ARMOR, SHIELD_REGEN, SHIELDS, SIZE, Unit, X,
    Y, ZERG_REGEN, by_tag, decode, speed,
};

/// `BWAPI::DamageType` and `UnitSizeType` ids.
const INDEPENDENT: i32 = 0;
const EXPLOSIVE: i32 = 1;
const CONCUSSIVE: i32 = 2;
const NORMAL: i32 = 3;
const IGNORE_ARMOR: i32 = 4;
const NO_DAMAGE: i32 = 5;
const SMALL: i32 = 1;
const MEDIUM: i32 = 2;
const LARGE: i32 = 3;

/// Frames of a duel.
const DUEL_FRAMES: i32 = 48;

/// A unit of a fight these properties make: as `tests/fight`'s fighter,
/// without speed, attack speed and range upgrades and stim.
#[derive(Clone, Copy, Debug)]
pub struct Body {
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
    pub elevation: i32,
}

/// Runs FAP, C++ or the port, on bodies, with tank splash or not, as
/// `openbw_host_fap_run`: the mode and the frames.
pub type Run<'a> = &'a dyn Fn(&[Body], bool, i32, i32) -> Vec<Vec<i32>>;

fn body(player: i32, unit_type: UnitType, x: i32) -> Body {
    Body {
        player,
        unit_type,
        x,
        y: 1000,
        health: unit_type.max_hit_points(),
        shields: unit_type.max_shields(),
        armor_upgrades: 0,
        attack_upgrades: 0,
        shield_upgrades: 0,
        elevation: -1,
    }
}

/// The unit FAP makes of a type without upgrades, as `tests/fight` adds it.
fn fap_unit(t: UnitType) -> bwapi_sdk::fap::Unit {
    let attackers = if t == UnitType::TERRAN_BUNKER { 4 } else { 8 };
    fap_unit_of_type(t, attackers, &FapUpgrades::default())
}

const SUICIDE: [UnitType; 4] = [
    UnitType::ZERG_SCOURGE,
    UnitType::TERRAN_VULTURE_SPIDER_MINE,
    UnitType::ZERG_INFESTED_TERRAN,
    UnitType::PROTOSS_SCARAB,
];

/// The duels: every attacker of `types` that has a weapon for every target of
/// `types`, but medics and interceptors, and suicide units as targets, which
/// leave by themselves; with upgrades of attack, armor and shields, the target
/// whole, hurt, with 1 or 3 shields, or without shields while the attacker
/// walks up, so that they regenerate below one.
pub fn duels(types: &[UnitType]) -> Vec<(String, Vec<Body>, bool)> {
    let skip = [UnitType::TERRAN_MEDIC, UnitType::PROTOSS_INTERCEPTOR];
    let upgrades = [(0, 0, 0), (3, 0, 0), (0, 3, 0), (0, 0, 3), (3, 3, 3)];
    let mut out = Vec::new();
    for &a in types.iter().filter(|t| !skip.contains(t)) {
        let unit = fap_unit(a);
        for &t in types
            .iter()
            .filter(|t| !skip.contains(t) && !SUICIDE.contains(t))
        {
            let (damage, min, max) = if t.is_flyer() {
                (unit.air_damage, unit.air_min_range, unit.air_max_range)
            } else {
                (
                    unit.ground_damage,
                    unit.ground_min_range,
                    unit.ground_max_range,
                )
            };
            if damage == 0 {
                continue;
            }
            let (hp, sh) = (t.max_hit_points(), t.max_shields());
            let mut states = vec![("whole", hp, sh, min), ("hurt", (hp / 2).max(1), 0, min)];
            if sh >= 3 {
                states.push(("1 shield", hp, 1, min));
                states.push(("3 shields", hp, 3, min));
            }
            if sh > 0 && unit.speed >= 1.0 {
                let walk = max + (unit.speed * 6.0) as i32;
                states.push(("shields regenerating", hp, 0, walk));
            }
            for &(attack, armor, shield) in &upgrades {
                for &(state, health, shields, distance) in &states {
                    let attacker = Body {
                        attack_upgrades: attack,
                        ..body(1, a, 1000)
                    };
                    let target = Body {
                        health,
                        shields,
                        armor_upgrades: armor,
                        shield_upgrades: shield,
                        ..body(2, t, 1000 + distance)
                    };
                    let name =
                        format!("duel {a:?} on {t:?}, {state}, upgrades {attack}/{armor}/{shield}");
                    out.push((name, vec![attacker, target], distance <= max));
                }
            }
        }
    }
    out
}

/// What a controlled fight shows; tag 0 is the attacker of player one.
#[derive(Clone, Debug)]
pub enum Expect {
    /// After the first frame, of player two only the unit `tag` is hurt; with
    /// `jump`, the attacker stands where it stood.
    Victim {
        property: &'static str,
        tag: i32,
        jump: bool,
    },
    /// No unit of player two is hurt and the attacker stepped toward `toward`.
    Step { toward: i32 },
    /// Tank splash around the target `main`.
    Splash { main: i32 },
    /// The suicide unit hit `hit` and left, or stepped toward the first enemy.
    Suicide { hit: Option<i32> },
    /// The attacker's cooldown after the first frame.
    Cooldown { expected: i32 },
    /// A bunker `bunker` the splash kills.
    NewMarines { bunker: i32 },
    /// A unit of the tank's side in its splash.
    Ally { ally: i32 },
}

pub struct Scenario {
    pub name: &'static str,
    pub bodies: Vec<Body>,
    pub splash: bool,
    pub expect: Expect,
}

/// The controlled fights, C1 to C8.
pub fn scenarios() -> Vec<Scenario> {
    use UnitType as T;
    let marine_range = fap_unit(T::TERRAN_MARINE).ground_max_range;
    let marine_cooldown = fap_unit(T::TERRAN_MARINE).ground_cooldown;
    let s = |name, bodies, splash, expect| Scenario {
        name,
        bodies,
        splash,
        expect,
    };
    let victim = |property, tag| Expect::Victim {
        property,
        tag,
        jump: false,
    };
    let uphill = |name, a: i32, t: i32, target: UnitType, expected: i32| {
        s(
            name,
            vec![
                Body {
                    elevation: a,
                    ..body(1, T::TERRAN_MARINE, 1000)
                },
                Body {
                    elevation: t,
                    ..body(2, target, 1050)
                },
            ],
            false,
            Expect::Cooldown { expected },
        )
    };
    let tank = || body(1, T::TERRAN_SIEGE_TANK_SIEGE_MODE, 1000);
    vec![
        s(
            "C1 the closest",
            vec![
                body(1, T::TERRAN_MARINE, 1000),
                body(2, T::PROTOSS_ZEALOT, 1100),
                body(2, T::PROTOSS_ZEALOT, 1060),
            ],
            false,
            victim("fap_targets_closest", 2),
        ),
        s(
            "C1 the first of two as close",
            vec![
                body(1, T::TERRAN_MARINE, 1000),
                body(2, T::PROTOSS_ZEALOT, 1060),
                body(2, T::PROTOSS_ZEALOT, 940),
            ],
            false,
            victim("fap_targets_closest", 1),
        ),
        s(
            "C1 not within the minimum range",
            vec![
                tank(),
                body(2, T::PROTOSS_ZEALOT, 1020),
                body(2, T::PROTOSS_ZEALOT, 1200),
            ],
            false,
            victim("fap_targets_closest", 2),
        ),
        s(
            "C1 only what its weapons hit",
            vec![
                body(1, T::PROTOSS_ZEALOT, 1000),
                body(2, T::ZERG_MUTALISK, 1010),
                body(2, T::TERRAN_MARINE, 1015),
            ],
            false,
            victim("fap_targets_closest", 2),
        ),
        s(
            "C2 at the range",
            vec![
                body(1, T::TERRAN_MARINE, 1000),
                body(2, T::PROTOSS_ZEALOT, 1000 + marine_range),
            ],
            false,
            victim("fap_attack_range", 1),
        ),
        s(
            "C2 past the range",
            vec![
                body(1, T::TERRAN_MARINE, 1000),
                body(2, T::PROTOSS_ZEALOT, 1001 + marine_range),
            ],
            false,
            Expect::Step { toward: 1 },
        ),
        s(
            "C3 nearer than a step",
            vec![
                body(1, T::PROTOSS_ZEALOT, 1000),
                body(2, T::TERRAN_MARINE, 1003),
            ],
            false,
            Expect::Victim {
                property: "fap_jump",
                tag: 1,
                jump: true,
            },
        ),
        s(
            "C4 splash",
            vec![
                tank(),
                body(2, T::PROTOSS_ZEALOT, 1200),
                body(2, T::TERRAN_MARINE, 1210),
                body(2, T::TERRAN_MARINE, 1240),
                body(2, T::TERRAN_MARINE, 1270),
                body(2, T::TERRAN_MARINE, 1300),
                body(2, T::ZERG_MUTALISK, 1205),
            ],
            true,
            Expect::Splash { main: 1 },
        ),
        s(
            "C5 scourge on a mutalisk",
            vec![
                body(1, T::ZERG_SCOURGE, 1000),
                body(2, T::ZERG_MUTALISK, 1005),
            ],
            false,
            Expect::Suicide { hit: Some(1) },
        ),
        s(
            "C5 scourge far from a mutalisk",
            vec![
                body(1, T::ZERG_SCOURGE, 1000),
                body(2, T::ZERG_MUTALISK, 1100),
            ],
            false,
            Expect::Suicide { hit: None },
        ),
        s(
            "C5 spider mine on a zealot",
            vec![
                body(1, T::TERRAN_VULTURE_SPIDER_MINE, 1000),
                body(2, T::PROTOSS_ZEALOT, 1010),
            ],
            false,
            Expect::Suicide { hit: Some(1) },
        ),
        s(
            "C5 infested terran on a marine",
            vec![
                body(1, T::ZERG_INFESTED_TERRAN, 1000),
                body(2, T::TERRAN_MARINE, 1004),
            ],
            false,
            Expect::Suicide { hit: Some(1) },
        ),
        s(
            "C5 scarab on a zealot",
            vec![
                body(1, T::PROTOSS_SCARAB, 1000),
                body(2, T::PROTOSS_ZEALOT, 1010),
            ],
            false,
            Expect::Suicide { hit: Some(1) },
        ),
        uphill(
            "C6 heights unknown",
            -1,
            -1,
            T::PROTOSS_ZEALOT,
            marine_cooldown - 1,
        ),
        uphill("C6 level", 0, 0, T::PROTOSS_ZEALOT, marine_cooldown - 1),
        uphill(
            "C6 uphill",
            0,
            1,
            T::PROTOSS_ZEALOT,
            2 * marine_cooldown - 1,
        ),
        uphill("C6 downhill", 1, 0, T::PROTOSS_ZEALOT, marine_cooldown - 1),
        uphill(
            "C6 uphill in the air",
            0,
            1,
            T::ZERG_MUTALISK,
            marine_cooldown - 1,
        ),
        s(
            "C7 a bunker the splash kills",
            vec![
                tank(),
                body(2, T::PROTOSS_ZEALOT, 1200),
                Body {
                    health: 1,
                    ..body(2, T::TERRAN_BUNKER, 1210)
                },
            ],
            true,
            Expect::NewMarines { bunker: 2 },
        ),
        s(
            "C8 a marine of the tank's side in its splash",
            vec![
                tank(),
                body(1, T::TERRAN_MARINE, 1210),
                body(2, T::ZERG_SPORE_COLONY, 1200),
            ],
            true,
            Expect::Ally { ally: 1 },
        ),
    ]
}

/// Health and shields after a hit of `damage` of `damage_type`, as FAP deals
/// it.
pub fn fap_hit(t: &[i32; FIELDS], damage: i32, damage_type: i32) -> (i32, i32) {
    let (mut health, mut shields) = (t[HEALTH], t[SHIELDS]);
    let mut d = damage << 8;
    let remaining = shields - d + (t[SHIELD_ARMOR] << 8);
    if remaining > 0 {
        return (health, remaining);
    } else if shields != 0 {
        d -= shields + (t[SHIELD_ARMOR] << 8);
        shields = 0;
    }
    if d == 0 {
        return (health, shields);
    }
    d -= t[ARMOR] << 8;
    let size = t[SIZE];
    if damage_type == CONCUSSIVE {
        if size == LARGE {
            d /= 4;
        } else if size == MEDIUM {
            d /= 2;
        }
    } else if damage_type == EXPLOSIVE {
        if size == SMALL {
            d /= 2;
        } else if size == MEDIUM {
            d = (d * 3) / 4;
        }
    }
    health -= d.max(128);
    (health, shields)
}

/// Health and shields after a hit as Brood War deals it: OpenBW
/// `weapon_deal_damage`, without hallucinations, acid spores and defensive
/// matrix.
pub fn bw_hit(t: &[i32; FIELDS], damage: i32, damage_type: i32) -> (i32, i32) {
    let mut d = (damage << 8).max(128);
    let mut shield_damage = 0;
    if t[MAX_SHIELDS] > 0 && t[SHIELDS] >= 256 {
        if damage_type != IGNORE_ARMOR {
            let armor = t[SHIELD_ARMOR] << 8;
            d = if d > armor { d - armor } else { 128 };
        }
        shield_damage = d.min(t[SHIELDS]);
        d -= shield_damage;
    }
    if damage_type != IGNORE_ARMOR {
        let armor = t[ARMOR] << 8;
        d = if d > armor { d - armor } else { 0 };
    }
    let size = t[SIZE];
    d = match damage_type {
        NO_DAMAGE => 0,
        EXPLOSIVE if size == INDEPENDENT => 0,
        EXPLOSIVE if size == SMALL => (d * 128) >> 8,
        EXPLOSIVE if size == MEDIUM => (d * 192) >> 8,
        CONCUSSIVE if size == INDEPENDENT => 0,
        CONCUSSIVE if size == MEDIUM => (d * 128) >> 8,
        CONCUSSIVE if size == LARGE => (d * 64) >> 8,
        _ => d,
    };
    if shield_damage == 0 && d < 128 {
        d = 128;
    }
    (t[HEALTH] - d, t[SHIELDS] - shield_damage)
}

/// Health and shields after regeneration at the end of a frame.
fn regen(kind: Kind, t: &[i32; FIELDS], (mut health, mut shields): (i32, i32)) -> (i32, i32) {
    if kind.regen_health && health < t[MAX_HEALTH] {
        health = (health + ZERG_REGEN).min(t[MAX_HEALTH]);
    }
    if kind.regen_shields && shields < t[MAX_SHIELDS] {
        shields = (shields + SHIELD_REGEN).min(t[MAX_SHIELDS]);
    }
    (health, shields)
}

/// The damage and its type of `a` on `t`, by the layer of `t`.
fn weapon(a: &[i32; FIELDS], t: &[i32; FIELDS]) -> (i32, i32) {
    if t[FLYING] != 0 {
        (a[AIR_DAMAGE], a[AIR_DAMAGE_TYPE])
    } else {
        (a[GROUND_DAMAGE], a[GROUND_DAMAGE_TYPE])
    }
}

/// Why FAP and Brood War differ on a hit: Δ1 to Δ4, 0 for another reason.
fn difference(
    t: &[i32; FIELDS],
    damage: i32,
    damage_type: i32,
    fap: (i32, i32),
    bw: (i32, i32),
) -> i32 {
    let shields = t[SHIELDS];
    if shields > 0 && shields < 256 {
        2
    } else if t[SIZE] == INDEPENDENT || damage_type == IGNORE_ARMOR || damage_type == NO_DAMAGE {
        4
    } else if shields > 0 && damage <= t[SHIELD_ARMOR] {
        3
    } else if shields >= 256 && t[HEALTH] - fap.0 == 128 && t[HEALTH] - bw.0 < 128 {
        1
    } else {
        0
    }
}

/// One step of `speed` from `from` toward `to`, as FAP takes it.
fn step(from: (i32, i32), to: (i32, i32), speed: f64) -> (i32, i32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = f64::from(dx * dx + dy * dy).sqrt();
    (
        from.0 + (f64::from(dx) * (speed / length)) as i32,
        from.1 + (f64::from(dy) * (speed / length)) as i32,
    )
}

fn at(u: &Unit) -> (i32, i32) {
    (u.fields[X], u.fields[Y])
}

/// The only unit of `side` with `tag`.
fn find(m: &Message, side: usize, tag: i32) -> Option<&Unit> {
    let mut found = m.units[side].iter().filter(|u| u.tag == tag);
    let first = found.next();
    if found.next().is_some() { None } else { first }
}

/// The violations of the mechanics: the duels and controlled fights run by
/// `run`, and `fights`, the random fights of [`super::fap`].
pub fn check(fights: &[Fight], types: &[UnitType], run: Run<'_>) -> Vec<Violation> {
    let mut report = Report::default();
    for (i, (name, bodies, in_range)) in duels(types).into_iter().enumerate() {
        check_duel(i as i32, &name, &bodies, in_range, run, &mut report);
    }
    for (i, scenario) in scenarios().iter().enumerate() {
        check_scenario(2_000_000 + i as i32, scenario, run, &mut report);
    }
    for fight in fights {
        check_random(fight, &mut report);
    }
    report.violations()
}

fn kinds(bodies: &[Body]) -> Vec<Kind> {
    bodies
        .iter()
        .map(|b| Kind::of(b.player, b.unit_type))
        .collect()
}

fn check_duel(
    id: i32,
    name: &str,
    bodies: &[Body],
    in_range: bool,
    run: Run<'_>,
    report: &mut Report,
) {
    let frames: Vec<Message> = run(bodies, false, FRAME_BY_FRAME, DUEL_FRAMES)
        .iter()
        .map(|m| decode(m))
        .collect();
    let target_kind = kinds(bodies)[1];
    for f in 1..frames.len() {
        let (before, after) = (&frames[f - 1], &frames[f]);
        let (Some(a), Some(t)) = (find(before, 0, 0), find(before, 1, 1)) else {
            break;
        };
        let fi = f as i32;
        let (a, t) = (&a.fields, &t.fields);
        let rest = regen(target_kind, t, (t[HEALTH], t[SHIELDS]));
        let marines = after.units[1].iter().filter(|u| u.tag == 1).count() > 1;
        let now = find(after, 1, 1).map(|u| (u.fields[HEALTH], u.fields[SHIELDS]));
        let changed = marines || now != Some(rest);
        let reset = a[COOLDOWN] == 0 && find(after, 0, 0).is_some_and(|u| u.fields[COOLDOWN] != 0);
        let key = |v: i32| (id, fi, v, 0);
        if !(changed || reset) {
            if f == 1 && in_range {
                report.add("fap_damage", key(0), || {
                    format!("{name}: no attack within range")
                });
            }
            continue;
        }
        let (damage, damage_type) = weapon(a, t);
        let hit = fap_hit(t, damage, damage_type);
        let expected = regen(target_kind, t, hit);
        let dead = hit.0 < 1;
        let ok = match now {
            _ if dead => now.is_none() || marines,
            Some(now) => now == expected,
            None => false,
        };
        if !ok {
            report.add("fap_damage", key(1), || {
                format!(
                    "{name}, frame {f}: health and shields {now:?}, FAP deals {expected:?} of {:?}",
                    (t[HEALTH], t[SHIELDS])
                )
            });
        }
        if let Some((health, shields)) = now
            && !marines
        {
            let total = t[HEALTH] + t[SHIELDS];
            let gain = i32::from(target_kind.regen_health) * ZERG_REGEN
                + i32::from(target_kind.regen_shields) * SHIELD_REGEN;
            // The loss lies between these: regeneration may hide some of it.
            let (least, most) = (total - health - shields, total - health - shields + gain);
            if most <= 0 {
                report.add("hit_takes_something", key(2), || {
                    format!(
                        "{name}, frame {f}: {:?} to {:?}",
                        (t[HEALTH], t[SHIELDS]),
                        (health, shields)
                    )
                });
            }
            if least > (damage << 8).max(128) {
                report.add("hit_within_damage", key(3), || {
                    format!("{name}, frame {f}: lost {least} of a hit of {damage}")
                });
            }
            let shields_left = shields - i32::from(target_kind.regen_shields) * SHIELD_REGEN;
            if shields_left > 0 && health < t[HEALTH] {
                report.add("shields_absorb_first", key(4), || {
                    format!(
                        "{name}, frame {f}: health {} to {health} with {shields} shields left",
                        t[HEALTH]
                    )
                });
            }
        }
        // A bunker, a reaver and a carrier deal the damage of their marines,
        // scarabs and interceptors with the damage type of their own weapon,
        // none, which FAP does not scale: in Brood War it is the normal
        // damage of those weapons.
        let bw_type = if damage_type == NO_DAMAGE {
            NORMAL
        } else {
            damage_type
        };
        let bw = bw_hit(t, damage, bw_type);
        let same = bw == hit || (bw.0 < 1 && dead && bw.1 == hit.1);
        if !same {
            let why = difference(t, damage, bw_type, hit, bw);
            report.add("diag_damage_differs_from_bw", (id, fi, why, 0), || {
                format!(
                    "{name}, frame {f}: Δ{why}, FAP {hit:?}, Brood War {bw:?} of {:?}",
                    (t[HEALTH], t[SHIELDS])
                )
            });
        }
    }
}

/// Hit points and shields of player two by tag after the first frame, and
/// whether each is as before but for regeneration.
fn hurt(bodies: &[Body], frames: &[Message]) -> BTreeMap<i32, bool> {
    let kinds = kinds(bodies);
    let mut out = BTreeMap::new();
    for u in &frames[0].units[1] {
        let rest = regen(
            kinds[u.tag as usize],
            &u.fields,
            (u.fields[HEALTH], u.fields[SHIELDS]),
        );
        let now = find(&frames[1], 1, u.tag).map(|v| (v.fields[HEALTH], v.fields[SHIELDS]));
        out.insert(u.tag, now != Some(rest));
    }
    out
}

fn check_scenario(id: i32, s: &Scenario, run: Run<'_>, report: &mut Report) {
    let first = |bodies: &[Body]| -> Vec<Message> {
        run(bodies, s.splash, FRAME_BY_FRAME, 1)
            .iter()
            .map(|m| decode(m))
            .collect()
    };
    let frames = first(&s.bodies);
    let name = s.name;
    if frames.len() < 2 {
        report.add("fap_damage", (id, 0, 0, 0), || {
            format!("{name}: no frame ran")
        });
        return;
    }
    let (before, after) = (&frames[0], &frames[1]);
    let hurt = hurt(&s.bodies, &frames);
    let attacker = find(before, 0, 0).expect("the attacker");
    match s.expect {
        Expect::Victim {
            property,
            tag,
            jump,
        } => {
            let victims: Vec<i32> = hurt.iter().filter(|(_, h)| **h).map(|(t, _)| *t).collect();
            let jumped = !jump || find(after, 0, 0).map(at) == find(before, 1, tag).map(at);
            if victims != [tag] || !jumped {
                report.add(property, (id, 0, 0, 0), || {
                    format!("{name}: hurt {victims:?}, expected {tag}; jumped onto it: {jumped}")
                });
            }
        }
        Expect::Step { toward } => {
            let target = find(before, 1, toward).expect("the target");
            let expected = step(at(attacker), at(target), speed(attacker));
            let now = find(after, 0, 0).map(at);
            if hurt.values().any(|h| *h) || now != Some(expected) {
                report.add("fap_attack_range", (id, 0, 0, 0), || {
                    format!("{name}: hurt {hurt:?}, the attacker at {now:?}, a step reaches {expected:?}")
                });
            }
        }
        Expect::Splash { main } => {
            check_splash(id, s, &frames, main, report);
            // FD8: the same health by unit in any order of player two.
            let n = s.bodies.iter().filter(|b| b.player == 2).count();
            let ones: Vec<Body> = s.bodies.iter().filter(|b| b.player == 1).copied().collect();
            let twos: Vec<Body> = s.bodies.iter().filter(|b| b.player == 2).copied().collect();
            let health = |order: &[usize]| -> Vec<Option<(i32, i32)>> {
                let mut bodies = ones.clone();
                bodies.extend(order.iter().map(|&i| twos[i]));
                let frames = first(&bodies);
                let mut by_body = vec![None; n];
                for (k, &i) in order.iter().enumerate() {
                    let tag = (ones.len() + k) as i32;
                    by_body[i] = frames
                        .get(1)
                        .and_then(|m| find(m, 1, tag))
                        .map(|u| (u.fields[HEALTH], u.fields[SHIELDS]));
                }
                by_body
            };
            let identity: Vec<usize> = (0..n).collect();
            let expected = health(&identity);
            let mut orders: Vec<Vec<usize>> = (1..n)
                .map(|r| (0..n).map(|i| (i + r) % n).collect())
                .collect();
            orders.push(identity.iter().rev().copied().collect());
            for order in orders {
                let got = health(&order);
                if got != expected {
                    report.add("fap_splash_order_independent", (id, 0, 0, 0), || {
                        format!(
                            "{name}: in the order {order:?}, {got:?}, in the given one {expected:?}"
                        )
                    });
                }
            }
        }
        Expect::Suicide { hit } => {
            let gone = find(after, 0, 0).is_none();
            let ok = match hit {
                Some(tag) => {
                    let t = &find(before, 1, tag).expect("the target").fields;
                    let (damage, damage_type) = weapon(&attacker.fields, t);
                    let kind = Kind::of(2, s.bodies[tag as usize].unit_type);
                    let expected = regen(kind, t, fap_hit(t, damage, damage_type));
                    let now = find(after, 1, tag).map(|u| (u.fields[HEALTH], u.fields[SHIELDS]));
                    gone && (now == Some(expected) || (expected.0 < 1 && now.is_none()))
                }
                None => {
                    let target = find(before, 1, 1).expect("the target");
                    let expected = step(at(attacker), at(target), speed(attacker));
                    !hurt.values().any(|h| *h) && find(after, 0, 0).map(at) == Some(expected)
                }
            };
            if !ok {
                report.add("fap_suicide", (id, 0, 0, 0), || {
                    format!("{name}: the suicide unit gone {gone}, hurt {hurt:?}")
                });
            }
        }
        Expect::Cooldown { expected } => {
            let now = find(after, 0, 0).map(|u| u.fields[COOLDOWN]);
            if now != Some(expected) {
                report.add("fap_uphill", (id, 0, 0, 0), || {
                    format!("{name}: cooldown {now:?}, expected {expected}")
                });
            }
        }
        Expect::NewMarines { bunker } => {
            // Only the tank acts on its side: marines hurt or fewer than the
            // bunker held were hit by the shot that killed it.
            let held = find(before, 1, bunker).map_or(0, |u| u.fields[ATTACKERS]);
            let marines: Vec<(i32, i32)> = after.units[1]
                .iter()
                .filter(|u| u.tag == bunker)
                .map(|u| (u.fields[HEALTH], u.fields[MAX_HEALTH]))
                .collect();
            if (marines.len() as i32) < held || marines.iter().any(|(h, max)| h < max) {
                report.add("diag_splash_hits_new_marines", (id, 0, 0, 0), || {
                    format!("{name}: of the {held} marines of the bunker, {marines:?} (health, most) are left after the shot")
                });
            }
        }
        Expect::Ally { ally } => {
            let then = find(before, 0, ally).map(|u| (u.fields[HEALTH], u.fields[SHIELDS]));
            let now = find(after, 0, ally).map(|u| (u.fields[HEALTH], u.fields[SHIELDS]));
            if then == now {
                report.add("diag_splash_spares_allies", (id, 0, 0, 0), || {
                    format!("{name}: the unit of the tank's side in its splash is not hurt; Brood War hurts it")
                });
            }
        }
    }
}

/// The ring of FAP's splash a unit at squared distance `d2` from the target is
/// in: 1 inner, 2 median, 3 outer, 0 none.
fn fap_ring(d2: i32, radii: [i32; 3]) -> usize {
    let effective = d2 / 4;
    radii
        .iter()
        .position(|&r| effective <= r * r)
        .map_or(0, |r| r + 1)
}

/// The ring of Brood War's splash: by the distance, as OpenBW's `xy_length`
/// measures it, from the impact at the target's center to the nearest point
/// of the unit's box.
fn bw_ring(impact: (i32, i32), u: (i32, i32), extent: [i32; 4], radii: [i32; 3]) -> usize {
    let [left, up, right, down] = extent;
    let nearest = (
        impact.0.clamp(u.0 - left, u.0 + right),
        impact.1.clamp(u.1 - up, u.1 + down),
    );
    let (mut x, mut y) = ((impact.0 - nearest.0).abs(), (impact.1 - nearest.1).abs());
    if x < y {
        std::mem::swap(&mut x, &mut y);
    }
    if x / 4 < y {
        x = x - x / 16 + y * 3 / 8 - x / 64 + y * 3 / 256;
    }
    radii.iter().position(|&r| x <= r).map_or(0, |r| r + 1)
}

/// FD7 and G8 on the first frame of a splash.
fn check_splash(id: i32, s: &Scenario, frames: &[Message], main: i32, report: &mut Report) {
    let name = s.name;
    let (before, after) = (&frames[0], &frames[1]);
    let tank = &find(before, 0, 0).expect("the tank").fields;
    let weapon = UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE.ground_weapon();
    let radii = [
        weapon.inner_splash_radius(),
        weapon.median_splash_radius(),
        weapon.outer_splash_radius(),
    ];
    let target = find(before, 1, main).expect("the target");
    let kinds = kinds(&s.bodies);
    for u in &before.units[1] {
        let t = &u.fields;
        let kind = kinds[u.tag as usize];
        let (dx, dy) = (t[X] - target.fields[X], t[Y] - target.fields[Y]);
        let ring = fap_ring(dx * dx + dy * dy, radii);
        let divisor = match ring {
            _ if u.tag == main => 1,
            _ if t[FLYING] != 0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 0,
        };
        let hit = if divisor == 0 {
            (t[HEALTH], t[SHIELDS])
        } else {
            fap_hit(t, tank[GROUND_DAMAGE] / divisor, tank[GROUND_DAMAGE_TYPE])
        };
        let expected = regen(kind, t, hit);
        let now = find(after, 1, u.tag).map(|v| (v.fields[HEALTH], v.fields[SHIELDS]));
        if now != Some(expected) && !(hit.0 < 1 && now.is_none()) {
            report.add("fap_splash", (id, u.tag, 0, 0), || {
                format!("{name}, unit {}: {now:?}, expected {expected:?} (ring {ring}, divisor {divisor})", u.tag)
            });
        }
        if u.tag != main && t[FLYING] == 0 {
            let theirs = bw_ring(at(target), at(u), kind.extent, radii);
            if theirs != ring {
                report.add(
                    "diag_splash_metric",
                    (id, u.tag, ring as i32, theirs as i32),
                    || {
                        format!(
                            "{name}, unit {}: ring {ring} in FAP, {theirs} in Brood War",
                            u.tag
                        )
                    },
                );
            }
        }
    }
}

/// I4, FD2, FD3 on the frames of a random fight.
fn check_random(fight: &Fight, report: &mut Report) {
    let (id, name) = (fight.id, &fight.name);
    let kind = |tag: i32| fight.kinds.get(tag as usize).copied().unwrap_or_default();
    for f in 1..fight.frames.len() {
        let (before, after) = (&fight.frames[f - 1], &fight.frames[f]);
        if after.outcome.is_some_and(|(_, frames)| frames == 0) {
            break;
        }
        let fi = f as i32;
        for side in 0..2 {
            let enemy = 1 - side;
            let attackers: Vec<&Unit> = before.units[enemy]
                .iter()
                .chain(&after.units[enemy])
                .collect();
            let (b, a) = (by_tag(&before.units[side]), by_tag(&after.units[side]));
            let enemy_died = {
                let (eb, ea) = (by_tag(&before.units[enemy]), by_tag(&after.units[enemy]));
                eb.iter()
                    .any(|(t, us)| ea.get(t).map_or(0, Vec::len) < us.len())
            };
            for (tag, now) in &a {
                let (Some(then), [v]) = (b.get(tag), now.as_slice()) else {
                    continue;
                };
                let [u] = then.as_slice() else { continue };
                let at_ = |m: &Unit| at(m);
                // I4, FD2.
                if v.fields[HEALTH] < u.fields[HEALTH] || v.fields[SHIELDS] < u.fields[SHIELDS] {
                    let flying = u.fields[FLYING] != 0;
                    let armed: Vec<&&Unit> = attackers
                        .iter()
                        .filter(|e| e.fields[if flying { AIR_DAMAGE } else { GROUND_DAMAGE }] > 0)
                        .collect();
                    if armed.is_empty() {
                        report.add("damage_needs_weapon", (id, fi, *tag, 0), || {
                            format!("{name}, frame {f}, side {side}, unit {tag}: hurt with no enemy armed for it")
                        });
                    }
                    let place = if side == 1 { at_(u) } else { at_(v) };
                    let reaches = armed.iter().any(|e| {
                        let range = f64::from(
                            e.fields[if flying {
                                AIR_MAX_RANGE
                            } else {
                                GROUND_MAX_RANGE
                            }],
                        )
                        .sqrt();
                        let splash = if flying { 0 } else { kind(e.tag).splash_reach };
                        let reach = speed(e) + range + f64::from(splash) + 1.0;
                        let (dx, dy) = (
                            f64::from(e.fields[X] - place.0),
                            f64::from(e.fields[Y] - place.1),
                        );
                        (dx * dx + dy * dy).sqrt() <= reach
                    });
                    if !armed.is_empty() && !reaches {
                        report.add("fap_damage_needs_reach", (id, fi, *tag, 0), || {
                            format!("{name}, frame {f}, side {side}, unit {tag}: hurt at {place:?} out of every enemy's reach")
                        });
                    }
                }
                // FD3.
                if at_(u) != at_(v) && !kind(*tag).medic && !(side == 1 && enemy_died) {
                    let places: Vec<(i32, i32)> = if side == 0 {
                        before.units[enemy]
                            .iter()
                            .chain(&after.units[enemy])
                            .map(at)
                            .collect()
                    } else {
                        after.units[enemy].iter().map(at).collect()
                    };
                    let ok = places
                        .iter()
                        .any(|&p| at_(v) == p || at_(v) == step(at_(u), p, speed(u)));
                    if !ok {
                        report.add("fap_move_rule", (id, fi, *tag, 0), || {
                            format!("{name}, frame {f}, side {side}, unit {tag}: {:?} to {:?}, toward no enemy", at_(u), at_(v))
                        });
                    }
                }
            }
        }
    }
}
