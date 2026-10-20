//! Properties of the state of FAP: bounds, the identity of units, what a frame
//! changes, when `simulate` stops, determinism. How damage, targets, reach,
//! splash and suicide work is not here.
//!
//! A fight is observed frame by frame, `simulate(1)` after `simulate(1)`; at
//! once, `simulate(k)`; again and on a cleared simulator. Each unit carries a
//! tag, its index among the fighters; the marines of a bunker carry the
//! bunker's. Hit points and shields are FAP's 1/256.
//!
//! A frame of FAP: every unit of player one acts, then every unit of player
//! two, then each unit's cooldown drops by one, its healing flag clears and it
//! regenerates. A bunker that dies turns into its marines in the turn of its
//! killer, at the end of its side.

use std::collections::BTreeMap;

use bwapi::{Race, UnitType};

use super::{Offenders, Violation, hash};

/// The fields of a unit, as `tests/fight` and `openbw-host` list them.
pub const FIELDS: usize = 27;
pub const X: usize = 0;
pub const Y: usize = 1;
pub const HEALTH: usize = 2;
pub const MAX_HEALTH: usize = 3;
pub const ARMOR: usize = 4;
pub const SHIELDS: usize = 5;
pub const MAX_SHIELDS: usize = 6;
pub const SHIELD_ARMOR: usize = 7;
pub const SPEED: usize = 8;
pub const SPEED_SQUARED: usize = 9;
pub const FLYING: usize = 10;
pub const GROUND_DAMAGE: usize = 12;
pub const GROUND_COOLDOWN: usize = 13;
pub const GROUND_MAX_RANGE: usize = 14;
pub const GROUND_DAMAGE_TYPE: usize = 16;
pub const AIR_DAMAGE: usize = 17;
pub const AIR_COOLDOWN: usize = 18;
pub const AIR_MAX_RANGE: usize = 19;
pub const AIR_DAMAGE_TYPE: usize = 21;
pub const SIZE: usize = 22;
pub const ORGANIC: usize = 23;
pub const HEALED: usize = 24;
pub const ATTACKERS: usize = 25;
pub const COOLDOWN: usize = 26;

/// What does not change while a unit lives.
const CONSTANT: [usize; 21] = [
    3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 25,
];

/// Regeneration and healing per frame, in 1/256.
pub const ZERG_REGEN: i32 = 4;
pub const SHIELD_REGEN: i32 = 7;
const HEAL: i32 = 150;

/// Reasons of `SimulationOutcome`.
const LIMIT_REACHED: i32 = 0;
const SIDE_ELIMINATED: i32 = 1;
const IDLE: i32 = 2;

/// Frames of a long run, and of the end of it in which no unit of an
/// unfinished fight may move.
pub const LONG_RUN: i32 = 7200;
const STALL: usize = 24;

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub fields: [i32; FIELDS],
    pub tag: i32,
}

/// What a call to `simulate` returned, and both players' units after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// Reason and frames; none for the state before any call.
    pub outcome: Option<(i32, i32)>,
    pub units: [Vec<Unit>; 2],
}

/// A message of `openbw_host_fap_run` (`openbw-host`'s
/// `cpp/fap_reference.cpp`): reason and frames, -1 for none, then per player
/// the count of units and per unit its fields and tag.
pub fn decode(raw: &[i32]) -> Message {
    let outcome = (raw[0] >= 0).then_some((raw[0], raw[1]));
    let mut rest = &raw[2..];
    let units = [0, 1].map(|_| {
        let (&n, tail) = rest.split_first().expect("a count of units");
        let (units, tail) = tail.split_at(n as usize * (FIELDS + 1));
        rest = tail;
        units
            .chunks_exact(FIELDS + 1)
            .map(|c| Unit {
                fields: c[..FIELDS].try_into().expect("the fields"),
                tag: c[FIELDS],
            })
            .collect()
    });
    Message { outcome, units }
}

/// What a fighter is, by its tag.
#[derive(Clone, Copy, Debug, Default)]
pub struct Kind {
    /// 0 or 1.
    pub side: usize,
    pub medic: bool,
    pub bunker: bool,
    /// Zerg but eggs and larvae.
    pub regen_health: bool,
    /// Protoss.
    pub regen_shields: bool,
    /// How much farther than its range its splash reaches, from the center of
    /// its target: twice the outer radius, as FAP measures; 0 but for a siege
    /// tank in siege mode.
    pub splash_reach: i32,
    /// Left, up, right and down of its center, in pixels.
    pub extent: [i32; 4],
}

impl Kind {
    /// Of a fighter of `player`, 1 or 2.
    pub fn of(player: i32, t: UnitType) -> Kind {
        let zerg = t.race() == Race::ZERG
            && ![
                UnitType::ZERG_EGG,
                UnitType::ZERG_LURKER_EGG,
                UnitType::ZERG_LARVA,
            ]
            .contains(&t);
        let splash_reach = if t == UnitType::TERRAN_SIEGE_TANK_SIEGE_MODE {
            2 * t.ground_weapon().outer_splash_radius() + 1
        } else {
            0
        };
        Kind {
            side: if player == 1 { 0 } else { 1 },
            medic: t == UnitType::TERRAN_MEDIC,
            bunker: t == UnitType::TERRAN_BUNKER,
            regen_health: zerg,
            regen_shields: t.race() == Race::PROTOSS,
            splash_reach,
            extent: [
                t.dimension_left(),
                t.dimension_up(),
                t.dimension_right(),
                t.dimension_down(),
            ],
        }
    }
}

/// The frames every fight is observed frame by frame, and the fights also
/// observed for [`LONG_RUN`] frames instead.
pub const FRAMES: i32 = 144;
pub const LONG_FIGHTS: u64 = 1000;

/// How a fight was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    Random,
    /// To show one thing: what the marines of a bunker are made with is exact.
    Controlled,
    /// And no unit can hit any enemy: regeneration and healing are exact.
    ControlledWithoutAttacks,
}

/// What is observed of a fight.
pub struct Fight {
    pub name: String,
    /// Names the fight in the hashes of offenders.
    pub id: i32,
    pub kinds: Vec<Kind>,
    /// Of a marine, in 1/256.
    pub marine_max_health: i32,
    pub made: Made,
    /// The frames asked for: frame by frame and at once.
    pub limit: i32,
    /// The state, then after each `simulate(1)` until one ran no frame.
    pub frames: Vec<Message>,
    /// Hashes of the frames run again, and on a simulator that ran the fight
    /// and was cleared; none where not run.
    pub again: Option<String>,
    pub reused: Option<String>,
    /// `simulate(limit)` at once.
    pub whole: Message,
    /// Where `whole` stopped idle after `j` frames: `j - 1` frames, then
    /// `simulate(2)`.
    pub idle_check: Option<Message>,
}

/// The modes of a run: see `openbw_host_fap_run`.
pub const FRAME_BY_FRAME: i32 = 0;
pub const REUSED: i32 = 1;
pub const AT_ONCE: i32 = 2;
pub const IDLE_CHECK: i32 = 3;

/// Hash of the messages of a run.
pub fn hash_of(raw: &[Vec<i32>]) -> String {
    hash(raw.iter().flatten().map(|&v| i64::from(v)))
}

/// Observes a fight through `run(mode, frames)`, which gives the messages of
/// FAP in C++ or of the port. But for a long run, it runs once more and on a
/// reused simulator.
pub fn observe(
    name: String,
    id: i32,
    kinds: Vec<Kind>,
    marine_max_health: i32,
    made: Made,
    limit: i32,
    run: impl Fn(i32, i32) -> Vec<Vec<i32>>,
) -> Fight {
    let raw = run(FRAME_BY_FRAME, limit);
    let frames: Vec<Message> = raw.iter().map(|m| decode(m)).collect();
    let (again_hash, reused) = if limit != LONG_RUN {
        (
            Some(hash_of(&run(FRAME_BY_FRAME, limit))),
            Some(hash_of(&run(REUSED, limit))),
        )
    } else {
        (None, None)
    };
    let whole = decode(&run(AT_ONCE, limit)[0]);
    let idle_check = match whole.outcome {
        Some((IDLE, j)) if j >= 1 => Some(decode(&run(IDLE_CHECK, j)[0])),
        _ => None,
    };
    let frames_hash = hash_of(&raw);
    Fight {
        name,
        id,
        kinds,
        marine_max_health,
        made,
        limit,
        frames,
        again: again_hash.map(|h| if h == frames_hash { String::new() } else { h }),
        reused: reused.map(|h| if h == frames_hash { String::new() } else { h }),
        whole,
        idle_check,
    }
}

/// The first difference of two observations of a fight, by FAP in C++ and by
/// the port: every message, in order, with the order of the units.
pub fn parity(port: &Fight, cpp: &Fight) -> Option<String> {
    if let Some(f) = (0..port.frames.len().max(cpp.frames.len()))
        .find(|&f| port.frames.get(f) != cpp.frames.get(f))
    {
        return Some(format!(
            "{}, frame {f}: port {:?}, C++ {:?}",
            port.name,
            port.frames.get(f),
            cpp.frames.get(f)
        ));
    }
    if port.whole != cpp.whole || port.idle_check != cpp.idle_check {
        return Some(format!(
            "{}: simulate({}) at once differs",
            port.name, port.limit
        ));
    }
    None
}

/// Offenders of the properties of FAP, by name.
#[derive(Default)]
pub struct Report {
    by: BTreeMap<&'static str, Offenders>,
}

impl Report {
    pub fn add(
        &mut self,
        property: &'static str,
        key: (i32, i32, i32, i32),
        detail: impl FnOnce() -> String,
    ) {
        self.by
            .entry(property)
            .or_insert_with(|| Offenders::new(property, "observations"))
            .add_keyed(key, detail);
    }

    /// A violation per property: its count of offenders and their hash.
    pub fn violations(self) -> Vec<Violation> {
        let mut out = Vec::new();
        let mut fail = |property: &'static str, subject: String, detail: String| {
            out.push(Violation {
                property: property.to_owned(),
                subject,
                detail,
            });
        };
        for (_, offenders) in self.by {
            offenders.report(&mut fail);
        }
        out
    }
}

/// The violations of the fights.
pub fn check(fights: &[Fight]) -> Vec<Violation> {
    let mut report = Report::default();
    for fight in fights {
        check_fight(fight, &mut report);
    }
    report.violations()
}

pub fn speed(u: &Unit) -> f64 {
    f64::from(f32::from_bits(u.fields[SPEED] as u32))
}

fn side_empty(m: &Message) -> bool {
    m.units[0].is_empty() || m.units[1].is_empty()
}

/// Cooldowns a frame may leave from `c`, of a unit with these weapons.
fn cooldown_after(c: i32, ground: i32, air: i32) -> Vec<i32> {
    if c > 0 {
        vec![c - 1]
    } else {
        vec![
            0,
            (air - 1).max(0),
            (ground - 1).max(0),
            (2 * ground - 1).max(0),
        ]
    }
}

/// A squared range of a bunker as its marines have it: a tile less.
fn marine_range(r: i32) -> i32 {
    let tiles = |t: i32| (t * 32) * (t * 32);
    if r == tiles(5) {
        tiles(4)
    } else if r == tiles(6) {
        tiles(5)
    } else {
        r
    }
}

fn check_fight(fight: &Fight, report: &mut Report) {
    let id = fight.id;
    let name = &fight.name;
    let kind = |tag: i32| fight.kinds.get(tag as usize).copied().unwrap_or_default();

    // F6: the box of the first positions.
    let start = fight
        .frames
        .first()
        .map(|m| m.units.iter().flatten().collect::<Vec<_>>());
    let bounds = start.as_ref().and_then(|units| {
        let xs = units.iter().map(|u| u.fields[X]);
        let ys = units.iter().map(|u| u.fields[Y]);
        Some((xs.clone().min()?, xs.max()?, ys.clone().min()?, ys.max()?))
    });

    let states = fight
        .frames
        .iter()
        .enumerate()
        .map(|(f, m)| (f as i32, m))
        .chain(std::iter::once((-1, &fight.whole)));
    for (f, m) in states {
        for (side, units) in m.units.iter().enumerate() {
            for u in units {
                let key = |v: i32| (id, f, u.tag, v);
                let at = || format!("{name}, frame {f}, side {side}, unit {}", u.tag);
                let v = &u.fields;
                if v[HEALTH] < 1 {
                    report.add("unit_alive", key(v[HEALTH]), || {
                        format!("{}: health {}", at(), v[HEALTH])
                    });
                }
                if v[HEALTH] > v[MAX_HEALTH] {
                    report.add("health_within_max", key(v[HEALTH]), || {
                        format!("{}: health {} of {}", at(), v[HEALTH], v[MAX_HEALTH])
                    });
                }
                if v[SHIELDS] < 0 || v[SHIELDS] > v[MAX_SHIELDS] {
                    report.add("shields_within_bounds", key(v[SHIELDS]), || {
                        format!("{}: shields {} of {}", at(), v[SHIELDS], v[MAX_SHIELDS])
                    });
                }
                if v[COOLDOWN] < 0 {
                    report.add("cooldown_not_negative", key(v[COOLDOWN]), || {
                        format!("{}: cooldown {}", at(), v[COOLDOWN])
                    });
                }
                let (s, s2) = (
                    f32::from_bits(v[SPEED] as u32),
                    f32::from_bits(v[SPEED_SQUARED] as u32),
                );
                if !(s.is_finite() && s >= 0.0 && s2.is_finite() && s2 >= 0.0) {
                    report.add("speed_finite", key(0), || {
                        format!("{}: speed {s}, squared {s2}", at())
                    });
                }
                if v[HEALED] != 0 && f != 0 {
                    report.add("fap_heal_flag_reset", key(0), || {
                        format!("{}: healed this frame", at())
                    });
                }
                if let Some((x0, x1, y0, y1)) = bounds
                    && (!(x0..=x1).contains(&v[X]) || !(y0..=y1).contains(&v[Y]))
                {
                    report.add("fap_positions_in_box", key(0), || {
                        format!(
                            "{}: at {},{}, the box {x0}..={x1}, {y0}..={y1}",
                            at(),
                            v[X],
                            v[Y]
                        )
                    });
                }
            }
        }
    }

    for f in 1..fight.frames.len() {
        check_frame(fight, f, report, &kind);
    }
    check_stop(fight, report);

    if let Some(h) = &fight.again
        && !h.is_empty()
    {
        report.add("fap_deterministic", (id, 0, 0, 0), || {
            format!("{name}: run again, hash {h}")
        });
    }
    if let Some(h) = &fight.reused
        && !h.is_empty()
    {
        report.add("simulator_reuse", (id, 0, 0, 0), || {
            format!("{name}: on a cleared simulator, hash {h}")
        });
    }
}

/// The units of a side by tag, in their order.
pub fn by_tag(units: &[Unit]) -> BTreeMap<i32, Vec<&Unit>> {
    let mut map: BTreeMap<i32, Vec<&Unit>> = BTreeMap::new();
    for u in units {
        map.entry(u.tag).or_default().push(u);
    }
    map
}

/// Whether the units after can be matched one to one with units before such
/// that `ok(before, after)` holds for each pair: marines of one bunker share a
/// tag.
fn matched(before: &[&Unit], after: &[&Unit], ok: &dyn Fn(&Unit, &Unit) -> bool) -> bool {
    fn assign(
        i: usize,
        before: &[&Unit],
        after: &[&Unit],
        used: &mut Vec<bool>,
        ok: &dyn Fn(&Unit, &Unit) -> bool,
    ) -> bool {
        if i == after.len() {
            return true;
        }
        for j in 0..before.len() {
            if !used[j] && ok(before[j], after[i]) {
                used[j] = true;
                if assign(i + 1, before, after, used, ok) {
                    return true;
                }
                used[j] = false;
            }
        }
        false
    }
    after.len() <= before.len() && assign(0, before, after, &mut vec![false; before.len()], ok)
}

fn check_frame(fight: &Fight, f: usize, report: &mut Report, kind: &dyn Fn(i32) -> Kind) {
    let (id, name) = (fight.id, &fight.name);
    let (before, after) = (&fight.frames[f - 1], &fight.frames[f]);
    let fi = f as i32;

    // F7b of each call: one frame, or none when a side is empty before.
    match after.outcome {
        Some((LIMIT_REACHED, 1)) if !side_empty(before) => {}
        Some((SIDE_ELIMINATED, 0)) if side_empty(before) && after.units == before.units => {}
        o => report.add("fap_stop_rule", (id, fi, 1, 0), || {
            format!(
                "{name}, frame {f}: simulate(1) gave {o:?}, a side empty before: {}",
                side_empty(before)
            )
        }),
    }
    if after.outcome.is_some_and(|(_, frames)| frames == 0) {
        return;
    }

    let no_attacks = fight.made == Made::ControlledWithoutAttacks;
    let expected = no_attacks.then(|| exact(before, kind));
    for side in 0..2 {
        let medics = before.units[side]
            .iter()
            .filter(|u| kind(u.tag).medic)
            .count();
        let (b, a) = (by_tag(&before.units[side]), by_tag(&after.units[side]));
        let vanished = b
            .iter()
            .filter(|(t, us)| a.get(t).map_or(0, Vec::len) < us.len())
            .count();
        for (&tag, after_units) in &a {
            let k = kind(tag);
            let at = || format!("{name}, frame {f}, side {side}, unit {tag}");
            let Some(before_units) = b.get(&tag) else {
                report.add("unit_identity", (id, fi, tag, 0), || {
                    format!("{}: not there before", at())
                });
                continue;
            };
            if k.side != side {
                report.add("unit_identity", (id, fi, tag, 1), || {
                    format!("{}: of the other side", at())
                });
            }
            let was = before_units[0];
            // A bunker turns into marines, known by their hit points, not by
            // their count: of one, into one.
            let is_marine = |u: &Unit| u.fields[MAX_HEALTH] == fight.marine_max_health;
            let bunker = k.bunker && before_units.len() == 1 && !is_marine(was);
            let marines = after_units.iter().filter(|u| is_marine(u)).count();
            if bunker && marines > 0 {
                let n = was.fields[ATTACKERS];
                if marines != after_units.len() {
                    report.add("unit_identity", (id, fi, tag, 2), || {
                        format!("{}: a bunker turned into marines and others", at())
                    });
                } else if after_units.len() as i32 > n {
                    report.add("unit_identity", (id, fi, tag, 2), || {
                        format!(
                            "{}: a bunker of {n} turned into {} marines",
                            at(),
                            after_units.len()
                        )
                    });
                }
                // No other unit of the side died if none vanished: the
                // bunker's tag grew.
                let only_death = vanished == 0;
                bunker_marines(
                    fight,
                    f,
                    (side, only_death),
                    was,
                    after_units,
                    &after.units[side],
                    report,
                );
                continue;
            }
            if after_units.len() > before_units.len() {
                report.add("unit_identity", (id, fi, tag, 3), || {
                    format!(
                        "{}: {} units, {} before",
                        at(),
                        after_units.len(),
                        before_units.len()
                    )
                });
                continue;
            }
            let mut pair_check = |property: &'static str,
                                  ok: &dyn Fn(&Unit, &Unit) -> bool,
                                  detail: &dyn Fn() -> String| {
                if !matched(before_units, after_units, ok) {
                    report.add(property, (id, fi, tag, 0), || {
                        format!("{}: {}", at(), detail())
                    });
                }
            };
            let first = (before_units[0], after_units[0]);
            pair_check(
                "unit_attributes_constant",
                &|u, v| CONSTANT.iter().all(|&i| u.fields[i] == v.fields[i]),
                &|| format!("{:?} to {:?}", first.0.fields, first.1.fields),
            );
            let regen = if k.regen_health { ZERG_REGEN } else { 0 };
            let healable = |u: &Unit| u.fields[ORGANIC] != 0 && medics > 0;
            let rise = |u: &Unit, v: &Unit| v.fields[HEALTH] - u.fields[HEALTH];
            let shield_rise = |u: &Unit, v: &Unit| v.fields[SHIELDS] - u.fields[SHIELDS];
            pair_check(
                "health_rises_by_regen_or_heal",
                &|u, v| {
                    (rise(u, v) <= 0 || k.regen_health || healable(u))
                        && (shield_rise(u, v) <= 0 || k.regen_shields)
                },
                &|| {
                    format!(
                        "health {} to {}, shields {} to {}",
                        first.0.fields[HEALTH],
                        first.1.fields[HEALTH],
                        first.0.fields[SHIELDS],
                        first.1.fields[SHIELDS]
                    )
                },
            );
            pair_check(
                "fap_regen",
                &|u, v| {
                    (healable(u) || rise(u, v) <= regen)
                        && shield_rise(u, v) <= if k.regen_shields { SHIELD_REGEN } else { 0 }
                },
                &|| {
                    format!(
                        "health {} to {}, shields {} to {}",
                        first.0.fields[HEALTH],
                        first.1.fields[HEALTH],
                        first.0.fields[SHIELDS],
                        first.1.fields[SHIELDS]
                    )
                },
            );
            pair_check(
                "fap_heal",
                &|u, v| !healable(u) || rise(u, v) <= regen + HEAL,
                &|| {
                    format!(
                        "health {} to {}",
                        first.0.fields[HEALTH], first.1.fields[HEALTH]
                    )
                },
            );
            let moved = |u: &Unit, v: &Unit| {
                let (dx, dy) = (
                    f64::from(v.fields[X] - u.fields[X]),
                    f64::from(v.fields[Y] - u.fields[Y]),
                );
                (dx * dx + dy * dy).sqrt()
            };
            if k.medic {
                pair_check(
                    "diag_medic_moves_beyond_speed",
                    &|u, v| moved(u, v) <= speed(u) + 1e-9,
                    &|| {
                        format!(
                            "moved {:.1}, speed {:.2}",
                            moved(first.0, first.1),
                            speed(first.0)
                        )
                    },
                );
                if medics == 1 {
                    pair_check(
                        "diag_medic_heals_itself",
                        &|u, v| rise(u, v) <= regen,
                        &|| {
                            format!(
                                "health {} to {}, no other medic",
                                first.0.fields[HEALTH], first.1.fields[HEALTH]
                            )
                        },
                    );
                    // M1: without attacks, regeneration or another medic.
                    if no_attacks && !k.regen_health {
                        pair_check("medic_does_not_heal_self", &|u, v| rise(u, v) <= 0, &|| {
                            format!(
                                "health {} to {}",
                                first.0.fields[HEALTH], first.1.fields[HEALTH]
                            )
                        });
                    }
                }
            } else {
                pair_check(
                    "unit_moves_within_speed",
                    &|u, v| moved(u, v) <= speed(u) + 1e-9,
                    &|| {
                        format!(
                            "moved {:.1}, speed {:.2}",
                            moved(first.0, first.1),
                            speed(first.0)
                        )
                    },
                );
                pair_check(
                    "fap_still_without_speed",
                    &|u, v| speed(u) != 0.0 || moved(u, v) == 0.0,
                    &|| format!("moved {:.1} at speed 0", moved(first.0, first.1)),
                );
            }
            pair_check(
                "fap_cooldown_step",
                &|u, v| {
                    cooldown_after(
                        u.fields[COOLDOWN],
                        u.fields[GROUND_COOLDOWN],
                        u.fields[AIR_COOLDOWN],
                    )
                    .contains(&v.fields[COOLDOWN])
                },
                &|| {
                    format!(
                        "cooldown {} to {}",
                        first.0.fields[COOLDOWN], first.1.fields[COOLDOWN]
                    )
                },
            );
            if let Some(expected) = &expected {
                for v in after_units {
                    if let Some(&(h, s)) = expected.get(&(side, tag)) {
                        if v.fields[HEALTH] != h {
                            let property = if healable(was) {
                                "fap_heal"
                            } else {
                                "fap_regen"
                            };
                            report.add(property, (id, fi, tag, 1), || {
                                format!(
                                    "{}: health {}, exactly {h} without attacks",
                                    at(),
                                    v.fields[HEALTH]
                                )
                            });
                        }
                        if v.fields[SHIELDS] != s {
                            report.add("fap_regen", (id, fi, tag, 2), || {
                                format!(
                                    "{}: shields {}, exactly {s} without attacks",
                                    at(),
                                    v.fields[SHIELDS]
                                )
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Health and shields after a frame without attacks: each medic, in its
/// side's order, heals the nearest wounded organic friend not healed yet and
/// stands on it; then all regenerate.
fn exact(before: &Message, kind: &dyn Fn(i32) -> Kind) -> BTreeMap<(usize, i32), (i32, i32)> {
    let mut out = BTreeMap::new();
    for side in 0..2 {
        let mut units: Vec<(Unit, bool)> = before.units[side]
            .iter()
            .map(|u| (u.clone(), false))
            .collect();
        for m in 0..units.len() {
            if !kind(units[m].0.tag).medic {
                continue;
            }
            let (mx, my) = (units[m].0.fields[X], units[m].0.fields[Y]);
            let mut best: Option<(usize, i64)> = None;
            for (i, (u, healed)) in units.iter().enumerate() {
                if u.fields[ORGANIC] != 0 && u.fields[HEALTH] < u.fields[MAX_HEALTH] && !healed {
                    let (dx, dy) = (i64::from(u.fields[X] - mx), i64::from(u.fields[Y] - my));
                    let d = dx * dx + dy * dy;
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((i, d));
                    }
                }
            }
            if let Some((i, _)) = best {
                let (x, y) = (units[i].0.fields[X], units[i].0.fields[Y]);
                units[m].0.fields[X] = x;
                units[m].0.fields[Y] = y;
                let p = &mut units[i];
                p.0.fields[HEALTH] = (p.0.fields[HEALTH] + HEAL).min(p.0.fields[MAX_HEALTH]);
                p.1 = true;
            }
        }
        for (u, _) in units {
            let k = kind(u.tag);
            let (mut h, mut s) = (u.fields[HEALTH], u.fields[SHIELDS]);
            if k.regen_health && h < u.fields[MAX_HEALTH] {
                h = (h + ZERG_REGEN).min(u.fields[MAX_HEALTH]);
            }
            if k.regen_shields && s < u.fields[MAX_SHIELDS] {
                s = (s + SHIELD_REGEN).min(u.fields[MAX_SHIELDS]);
            }
            out.insert((side, u.tag), (h, s));
        }
    }
    out
}

/// F5: the marines a bunker turned into, at the end of the frame, by the
/// bunker's side and whether no other unit of that side died. What an attack
/// does not change is as made. Units of the killer's side that act later in
/// the turn may hurt the marines, and so may the rest of the splash that
/// killed the bunker: their hit points are as made only in controlled fights.
/// Their cooldown is as made where they did not act yet, the bunker of player
/// one; their place at the end of the side where no other unit of the side
/// died. Shields are left to `shields_within_bounds`.
fn bunker_marines(
    fight: &Fight,
    f: usize,
    (side, only_death): (usize, bool),
    bunker: &Unit,
    marines: &[&Unit],
    units: &[Unit],
    report: &mut Report,
) {
    let b = &bunker.fields;
    let n = b[ATTACKERS];
    let ground_range = marine_range(b[GROUND_MAX_RANGE]);
    let mut expected = *b;
    expected[HEALTH] = fight.marine_max_health;
    expected[MAX_HEALTH] = fight.marine_max_health;
    expected[ARMOR] = 0;
    expected[GROUND_COOLDOWN] = b[GROUND_COOLDOWN] * n;
    expected[AIR_COOLDOWN] = b[AIR_COOLDOWN] * n;
    expected[GROUND_MAX_RANGE] = ground_range;
    expected[AIR_MAX_RANGE] = ground_range;
    expected[HEALED] = 0;
    let cooldowns = cooldown_after(b[COOLDOWN], b[GROUND_COOLDOWN], b[AIR_COOLDOWN]);
    for m in marines {
        let differs: Vec<usize> = (0..FIELDS)
            .filter(|&i| match i {
                HEALTH if fight.made != Made::Random => m.fields[i] != expected[i],
                HEALTH => !(1..=expected[MAX_HEALTH]).contains(&m.fields[i]),
                SHIELDS => false,
                COOLDOWN => side == 0 && !cooldowns.contains(&m.fields[i]),
                _ => m.fields[i] != expected[i],
            })
            .collect();
        if !differs.is_empty() {
            report.add("fap_bunker_marines", (fight.id, f as i32, bunker.tag, differs[0] as i32), || {
                format!(
                    "{}, frame {f}, side {side}, unit {}: fields {differs:?} of {:?}, expected {:?}",
                    fight.name, bunker.tag, m.fields, expected
                )
            });
        }
    }
    let tail = &units[units.len().saturating_sub(marines.len())..];
    if only_death && !tail.iter().all(|u| u.tag == bunker.tag) {
        report.add(
            "fap_bunker_marines",
            (fight.id, f as i32, bunker.tag, -1),
            || {
                format!(
                    "{}, frame {f}, side {side}: the marines of {} not at the end",
                    fight.name, bunker.tag
                )
            },
        );
    }
}

/// F7a, F7b of `simulate(limit)` at once; G3, G4.
fn check_stop(fight: &Fight, report: &mut Report) {
    let (id, name) = (fight.id, &fight.name);
    let Some((reason, j)) = fight.whole.outcome else {
        return;
    };
    let ju = j as usize;
    let at_j = fight.frames.get(ju);
    match at_j {
        Some(m) if m.units == fight.whole.units => {}
        _ => report.add("fap_frames_compose", (id, j, 0, 0), || {
            format!(
                "{name}: simulate({}) ran {j} frames, unlike {j} times simulate(1)",
                fight.limit
            )
        }),
    }
    let ok = match reason {
        LIMIT_REACHED => j == fight.limit,
        SIDE_ELIMINATED => j < fight.limit && at_j.is_some_and(side_empty),
        IDLE => {
            j < fight.limit
                && j >= 1
                && at_j.is_some_and(|m| !side_empty(m))
                && fight.idle_check.as_ref().and_then(|m| m.outcome) == Some((IDLE, 1))
        }
        _ => false,
    };
    if !ok {
        report.add("fap_stop_rule", (id, j, reason, 0), || {
            format!(
                "{name}: simulate({}) gave reason {reason} after {j} frames; simulate(2) after {} frames gave {:?}",
                fight.limit,
                j - 1,
                fight.idle_check.as_ref().and_then(|m| m.outcome)
            )
        });
    }
    // G3: healing still to do when it stopped idle.
    if reason == IDLE
        && let Some(m) = at_j
    {
        for side in 0..2 {
            let medic = m.units[side]
                .iter()
                .any(|u| fight.kinds.get(u.tag as usize).is_some_and(|k| k.medic));
            let wounded = m.units[side]
                .iter()
                .any(|u| u.fields[ORGANIC] != 0 && u.fields[HEALTH] < u.fields[MAX_HEALTH]);
            if medic && wounded {
                report.add("diag_heal_stops_simulation", (id, j, side as i32, 0), || {
                    format!("{name}: stopped idle after {j} frames, side {side} has a medic and wounded")
                });
            }
        }
    }
    // G4a, G4b: by the states only, which do not show that a unit tried to
    // move.
    if fight.limit == LONG_RUN && reason == LIMIT_REACHED && !side_empty(&fight.whole) {
        report.add("diag_fight_unfinished", (id, 0, 0, 0), || {
            format!("{name}: both sides stand after {LONG_RUN} frames")
        });
        let n = fight.frames.len();
        if n > STALL {
            let (then, now) = (&fight.frames[n - 1 - STALL], &fight.frames[n - 1]);
            let still = (0..2).all(|s| {
                then.units[s].len() == now.units[s].len()
                    && then.units[s].iter().zip(&now.units[s]).all(|(u, v)| {
                        u.tag == v.tag && (u.fields[X], u.fields[Y]) == (v.fields[X], v.fields[Y])
                    })
            });
            if still {
                report.add("diag_positions_unchanged", (id, 0, 0, 0), || {
                    format!("{name}: no unit moved in the last {STALL} frames")
                });
            }
        }
    }
}
