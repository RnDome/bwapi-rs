// Port of FAP/include/FAP.hpp of FAP (https://github.com/N00byEdge/FAP, ac3c005).
// Copyright (c) 2018 Hannes Bredberg. MIT License, see LICENSE-FAP.

//! FAP, the Fast APproximation of a fight between two groups of units: an
//! unofficial port of FAP by Hannes Bredberg.
//!
//! Each frame every unit of the first side acts, then every unit of the second,
//! then cooldowns tick and units regenerate. A unit attacks the closest enemy it
//! can hit, walking straight to it; units are points and ranges are measured
//! between centers. A run stops after the given number of frames, when a side is
//! empty or when a frame changed nothing.
//!
//! Differences from the C++ code:
//! - the `unitType` of a unit is replaced by what FAP decides by it: [`Role`],
//!   [`Regeneration`] and [`Splash`];
//! - [`Unit`] is a struct with the values of the `FAP::Unit` setters; there is
//!   no typestate builder, and the unit types and upgrades are resolved by the
//!   caller;
//! - the simulator owns all its memory from [`Simulator::with_capacity`] on:
//!   adding more units than it holds is an error, and neither adding nor
//!   running allocates;
//! - a tank splash kill that moves the tank's target in the vector keeps
//!   tracking the target; C++ keeps an iterator to the slot it left;
//! - the API is renamed: a fight is a [`Battle`] of the [`Simulator`],
//!   `FAP::FastAPproximation`, which starts empty instead of after `clear`;
//!   `addIfCombatUnitPlayer1/2` is [`Battle::add`] with a [`Side`],
//!   `simulate<tankSplash>` is [`Battle::run`] of a battle with [`Options`],
//!   `getState` is [`Battle::units`] and [`Battle::units_mut`];
//! - `run` takes a count of frames that is not negative: C++ `simulate(-1)`
//!   runs until the fight ends by itself, as `run(u32::MAX)` does for a fight
//!   that ends sooner than that;
//! - `run` tells how many frames it ran and why it stopped: [`Outcome`].

mod unit;

pub use unit::{DamageType, FapUnit, Regeneration, Role, Splash, Unit, UnitSizeType};

use alloc::vec::Vec;
use core::fmt;

use crate::math::sqrt;

/// Frames `simulate` runs in C++ by default: 4 seconds on the fastest speed.
pub const DEFAULT_FRAMES: u32 = 96;

/// One of the two sides. The first acts first in every frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    First,
    Second,
}

/// How a [`Battle`] runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Units with a [`Splash`] (siege tanks) splash: `simulate<true>` in C++.
    pub tank_splash: bool,
}

/// Why [`Battle::run`] stopped, and after how many frames it ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// It ran the frames it was given, whatever the last one left.
    LimitReached { frames: u32 },
    /// A side had no units before the next frame; no frame at all if one had
    /// none at the start.
    SideEliminated { frames: u32 },
    /// Nothing happened in the last frame it ran, which counts.
    Idle { frames: u32 },
}

impl Outcome {
    /// The frames it ran.
    pub fn frames(self) -> u32 {
        match self {
            Outcome::LimitReached { frames }
            | Outcome::SideEliminated { frames }
            | Outcome::Idle { frames } => frames,
        }
    }
}

/// A side has no room for one more unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacityExceeded;

impl fmt::Display for CapacityExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the simulator holds no more units for this side")
    }
}

impl core::error::Error for CapacityExceeded {}

/// The simulator, `FAP::FastAPproximation`: the memory of the battles it runs,
/// one at a time.
#[derive(Clone, Debug)]
pub struct Simulator<T = ()> {
    first: Vec<FapUnit<T>>,
    second: Vec<FapUnit<T>>,
    /// Slots each side may need: a unit and the marines its bunker turns into.
    reserved: [usize; 2],
}

impl<T: Clone> Simulator<T> {
    /// A simulator for up to `units_per_side` units of each side, counting
    /// every marine a bunker turns into.
    pub fn with_capacity(units_per_side: usize) -> Self {
        Simulator {
            first: Vec::with_capacity(units_per_side),
            second: Vec::with_capacity(units_per_side),
            reserved: [0; 2],
        }
    }

    /// A new battle, with no units: those of the last battle are gone, the
    /// memory stays.
    pub fn battle(&mut self, options: Options) -> Battle<'_, T> {
        self.first.clear();
        self.second.clear();
        self.reserved = [0; 2];
        Battle {
            simulator: self,
            options,
        }
    }
}

/// A fight between two sides, run frame by frame.
#[derive(Debug)]
pub struct Battle<'s, T = ()> {
    simulator: &'s mut Simulator<T>,
    options: Options,
}

impl<T: Clone> Battle<'_, T> {
    /// Adds the unit to `side` if it is a combat unit: it has a weapon or is a
    /// medic, and its role is not [`Role::Ignored`]; otherwise does nothing.
    pub fn add(&mut self, side: Side, unit: Unit<T>) -> Result<(), CapacityExceeded> {
        let unit = FapUnit::from(unit);
        if !is_combat_unit(&unit) {
            return Ok(());
        }
        let simulator = &mut *self.simulator;
        let (units, side) = match side {
            Side::First => (&mut simulator.first, 0),
            Side::Second => (&mut simulator.second, 1),
        };
        let slots = match unit.role {
            Role::Bunker { .. } if unit.attacker_count > 0 => unit.attacker_count as usize,
            _ => 1,
        };
        let reserved = simulator.reserved[side] + slots;
        if reserved > units.capacity() {
            return Err(CapacityExceeded);
        }
        simulator.reserved[side] = reserved;
        units.push(unit);
        Ok(())
    }

    /// Runs at most `frames` frames ([`DEFAULT_FRAMES`] in C++). Can be called
    /// again to go on. C++ returns nothing.
    pub fn run(&mut self, frames: u32) -> Outcome {
        if self.options.tank_splash {
            self.run_frames::<true>(frames)
        } else {
            self.run_frames::<false>(frames)
        }
    }

    /// The units of `side`.
    pub fn units(&self, side: Side) -> &[FapUnit<T>] {
        match side {
            Side::First => &self.simulator.first,
            Side::Second => &self.simulator.second,
        }
    }

    /// The units of `side`, to edit between runs.
    pub fn units_mut(&mut self, side: Side) -> &mut [FapUnit<T>] {
        match side {
            Side::First => &mut self.simulator.first,
            Side::Second => &mut self.simulator.second,
        }
    }

    fn run_frames<const TANK_SPLASH: bool>(&mut self, mut remaining: u32) -> Outcome {
        let mut frames: u32 = 0;
        while remaining != 0 {
            remaining -= 1;
            if self.simulator.first.is_empty() || self.simulator.second.is_empty() {
                return Outcome::SideEliminated { frames };
            }
            let did_something = self.simulate_frame::<TANK_SPLASH>();
            frames = frames.wrapping_add(1);
            if !did_something && remaining != 0 {
                return Outcome::Idle { frames };
            }
        }
        Outcome::LimitReached { frames }
    }

    /// `isimulate`: one frame. Whether a unit did something.
    fn simulate_frame<const TANK_SPLASH: bool>(&mut self) -> bool {
        let simulator = &mut *self.simulator;
        let mut did_something = false;
        let did = &mut did_something;
        simulate_side::<T, TANK_SPLASH>(&mut simulator.first, &mut simulator.second, did);
        simulate_side::<T, TANK_SPLASH>(&mut simulator.second, &mut simulator.first, did);

        for unit in simulator
            .first
            .iter_mut()
            .chain(simulator.second.iter_mut())
        {
            update_unit(unit);
        }
        did_something
    }
}

/// `simUnit` of `isimulate` for every unit of one side.
fn simulate_side<T: Clone, const TANK_SPLASH: bool>(
    friendly: &mut Vec<FapUnit<T>>,
    enemy: &mut Vec<FapUnit<T>>,
    did_something: &mut bool,
) {
    let mut i = 0;
    while i < friendly.len() {
        match friendly[i].role {
            Role::Suicide => {
                if simulate_suicide(&mut friendly[i], enemy, did_something) {
                    friendly.remove(i);
                } else {
                    i += 1;
                }
            }
            Role::Medic => {
                simulate_medic(i, friendly);
                i += 1;
            }
            _ => {
                simulate_fighter::<T, TANK_SPLASH>(&mut friendly[i], enemy, did_something);
                i += 1;
            }
        }
    }
}

/// `updateUnit` of `isimulate`: cooldown, healing flag, regeneration.
fn update_unit<T>(unit: &mut FapUnit<T>) {
    if unit.attack_cooldown_remaining != 0 {
        unit.attack_cooldown_remaining -= 1;
    }
    unit.did_heal_this_frame = false;

    match unit.regeneration {
        Regeneration::ZergHealth => {
            if unit.health < unit.max_health {
                unit.health += 4;
            }
            if unit.health > unit.max_health {
                unit.health = unit.max_health;
            }
        }
        Regeneration::ProtossShields => {
            if unit.shields < unit.max_shields {
                unit.shields += 7;
            }
            if unit.shields > unit.max_shields {
                unit.shields = unit.max_shields;
            }
        }
        Regeneration::None => {}
    }
}

fn is_combat_unit<T>(u: &FapUnit<T>) -> bool {
    u.role != Role::Ignored && (u.air_damage != 0 || u.ground_damage != 0 || u.role == Role::Medic)
}

fn deal_damage<T>(unit: &mut FapUnit<T>, damage: i32, damage_type: DamageType) {
    let mut damage = damage << 8;
    let remaining_shields = unit.shields - damage + (unit.shield_armor << 8);
    if remaining_shields > 0 {
        unit.shields = remaining_shields;
        return;
    } else if unit.shields != 0 {
        damage -= unit.shields + (unit.shield_armor << 8);
        unit.shields = 0;
    }

    if damage == 0 {
        return;
    }

    damage -= unit.armor << 8;

    if damage_type == DamageType::Concussive {
        if unit.unit_size == UnitSizeType::Large {
            damage /= 4;
        } else if unit.unit_size == UnitSizeType::Medium {
            damage /= 2;
        }
    } else if damage_type == DamageType::Explosive {
        if unit.unit_size == UnitSizeType::Small {
            damage /= 2;
        } else if unit.unit_size == UnitSizeType::Medium {
            damage = (damage * 3) / 4;
        }
    }

    unit.health -= damage.max(128);
}

fn distance_squared<T>(u1: &FapUnit<T>, u2: &FapUnit<T>) -> i32 {
    let (dx, dy) = (u1.x.wrapping_sub(u2.x), u1.y.wrapping_sub(u2.y));
    dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))
}

/// The enemy closest to `unit` that `unit` has a weapon for, not inside its minimum
/// range; the first one on ties.
fn closest_enemy<T>(unit: &FapUnit<T>, enemy_units: &[FapUnit<T>]) -> Option<(usize, i32)> {
    let mut closest: Option<(usize, i32)> = None;
    for (i, enemy) in enemy_units.iter().enumerate() {
        let (damage, min_range_squared) = if enemy.flying {
            (unit.air_damage, unit.air_min_range_squared)
        } else {
            (unit.ground_damage, unit.ground_min_range_squared)
        };
        if damage != 0 {
            let d = distance_squared(unit, enemy);
            if closest.is_none_or(|(_, cd)| d < cd) && d >= min_range_squared {
                closest = Some((i, d));
            }
        }
    }
    closest
}

/// One step of `speed` pixels straight toward `(tx, ty)`.
fn step_toward<T>(unit: &mut FapUnit<T>, tx: i32, ty: i32) {
    let dx = tx - unit.x;
    let dy = ty - unit.y;
    let length = sqrt(f64::from(dx * dx + dy * dy));
    unit.x += (f64::from(dx) * (f64::from(unit.speed) / length)) as i32;
    unit.y += (f64::from(dy) * (f64::from(unit.speed) / length)) as i32;
}

/// Removes the dead unit at `index` by swapping in the last one.
fn remove_dead<T: Clone>(index: usize, units: &mut Vec<FapUnit<T>>) {
    let dead = units.swap_remove(index);
    unit_death(dead, units);
}

/// `unitsim`.
fn simulate_fighter<T: Clone, const TANK_SPLASH: bool>(
    unit: &mut FapUnit<T>,
    enemy_units: &mut Vec<FapUnit<T>>,
    did_something: &mut bool,
) {
    if unit.attack_cooldown_remaining != 0 {
        *did_something = true;
        return;
    }

    if unit.ground_damage == 0 && unit.air_damage == 0 {
        return;
    }

    let Some((mut closest, mut closest_distance_squared)) = closest_enemy(unit, enemy_units) else {
        return;
    };

    if closest_distance_squared as f32 <= unit.speed_squared
        && !(unit.x == enemy_units[closest].x && unit.y == enemy_units[closest].y)
    {
        unit.x = enemy_units[closest].x;
        unit.y = enemy_units[closest].y;
        closest_distance_squared = 0;

        *did_something = true;
    }

    let target_flying = enemy_units[closest].flying;
    let max_range_squared = if target_flying {
        unit.air_max_range_squared
    } else {
        unit.ground_max_range_squared
    };

    if closest_distance_squared <= max_range_squared {
        if target_flying {
            deal_damage(
                &mut enemy_units[closest],
                unit.air_damage,
                unit.air_damage_type,
            );
            unit.attack_cooldown_remaining = unit.air_cooldown;
        } else {
            deal_damage(
                &mut enemy_units[closest],
                unit.ground_damage,
                unit.ground_damage_type,
            );

            if TANK_SPLASH && let Some(splash) = unit.splash {
                closest = tank_splash(unit, splash, closest, enemy_units);
            }

            let target = &enemy_units[closest];
            let higher =
                unit.elevation != -1 && target.elevation != -1 && target.elevation > unit.elevation;
            unit.attack_cooldown_remaining = unit.ground_cooldown << i32::from(higher);
        }

        if enemy_units[closest].health < 1 {
            remove_dead(closest, enemy_units);
        }

        *did_something = true;
    } else if closest_distance_squared as f32 > unit.speed_squared && unit.speed >= 1.0 {
        let (tx, ty) = (enemy_units[closest].x, enemy_units[closest].y);
        step_toward(unit, tx, ty);

        *did_something = true;
    }
}

/// Splash of a siege tank around its target at `closest`, which keeps its
/// health: it is checked by the caller. Returns where the target is now.
fn tank_splash<T: Clone>(
    unit: &FapUnit<T>,
    splash: Splash,
    mut closest: usize,
    enemy_units: &mut Vec<FapUnit<T>>,
) -> usize {
    let inner_squared = splash.inner * splash.inner;
    let median_squared = splash.median * splash.median;
    let outer_squared = splash.outer * splash.outer;

    let mut enemy_index = 0;
    while enemy_index < enemy_units.len() {
        if enemy_units[enemy_index].flying || enemy_index == closest {
            enemy_index += 1;
            continue;
        }
        // Shell hit point to unit edge.
        let effective_distance_squared =
            distance_squared(&enemy_units[closest], &enemy_units[enemy_index]) / 4;
        let damage = if effective_distance_squared <= inner_squared {
            Some(unit.ground_damage)
        } else if effective_distance_squared <= median_squared {
            Some(unit.ground_damage / 2)
        } else if effective_distance_squared <= outer_squared {
            Some(unit.ground_damage / 4)
        } else {
            None
        };

        let mut killed = false;
        if let Some(damage) = damage {
            deal_damage(
                &mut enemy_units[enemy_index],
                damage,
                unit.ground_damage_type,
            );
            if enemy_units[enemy_index].health < 1 {
                killed = true;
                if closest == enemy_units.len() - 1 {
                    closest = enemy_index;
                }
                remove_dead(enemy_index, enemy_units);
            }
        }
        if !killed {
            enemy_index += 1;
        }
    }
    closest
}

/// `medicsim`.
fn simulate_medic<T>(medic: usize, friendly_units: &mut [FapUnit<T>]) {
    let unit = &friendly_units[medic];
    let mut closest_healable: Option<(usize, i32)> = None;

    for (i, it) in friendly_units.iter().enumerate() {
        if it.is_organic && it.health < it.max_health && !it.did_heal_this_frame {
            let d = distance_squared(unit, it);
            if closest_healable.is_none_or(|(_, cd)| d < cd) {
                closest_healable = Some((i, d));
            }
        }
    }

    if let Some((i, _)) = closest_healable {
        let (x, y) = (friendly_units[i].x, friendly_units[i].y);
        friendly_units[medic].x = x;
        friendly_units[medic].y = y;

        let healed = &mut friendly_units[i];
        healed.health += 150;
        if healed.health > healed.max_health {
            healed.health = healed.max_health;
        }
        healed.did_heal_this_frame = true;
    }
}

/// `suicideSim`: whether the unit hit and died.
fn simulate_suicide<T: Clone>(
    unit: &mut FapUnit<T>,
    enemy_units: &mut Vec<FapUnit<T>>,
    did_something: &mut bool,
) -> bool {
    let Some((closest, closest_distance_squared)) = closest_enemy(unit, enemy_units) else {
        return false;
    };

    if closest_distance_squared as f32 <= unit.speed_squared {
        let target = &mut enemy_units[closest];
        if target.flying {
            deal_damage(target, unit.air_damage, unit.air_damage_type);
        } else {
            deal_damage(target, unit.ground_damage, unit.ground_damage_type);
        }

        if target.health < 1 {
            remove_dead(closest, enemy_units);
        }

        *did_something = true;
        true
    } else {
        let (tx, ty) = (enemy_units[closest].x, enemy_units[closest].y);
        step_toward(unit, tx, ty);

        *did_something = true;
        false
    }
}

/// A dead bunker with marines turns into its marines.
fn unit_death<T: Clone>(mut unit: FapUnit<T>, friendlies: &mut Vec<FapUnit<T>>) {
    let Role::Bunker { marine_max_health } = unit.role else {
        return;
    };
    if unit.attacker_count == 0 {
        return;
    }
    unit.role = Role::Fighter;

    const fn squared_range(tiles: i32) -> i32 {
        (tiles * 32) * (tiles * 32)
    }
    match unit.ground_max_range_squared {
        // No range upgrade.
        r if r == squared_range(5) => unit.ground_max_range_squared = squared_range(4),
        // Range upgrade.
        r if r == squared_range(6) => unit.ground_max_range_squared = squared_range(5),
        _ => {}
    }

    unit.air_max_range_squared = unit.ground_max_range_squared;

    unit.armor = 0;

    unit.max_health = marine_max_health << 8;
    unit.health = unit.max_health;

    unit.ground_cooldown *= unit.attacker_count;
    unit.air_cooldown *= unit.attacker_count;

    for _ in 0..unit.attacker_count - 1 {
        friendlies.push(unit.clone());
    }
    friendlies.push(unit);
}
