// Port of FAP/include/FAP.hpp of FAP (https://github.com/N00byEdge/FAP, ac3c005).
// Copyright (c) 2018 Hannes Bredberg. MIT License, see LICENSE-FAP.

//! FAP, the Fast APproximation of a fight between two groups of units: an
//! unofficial port of FAP by Hannes Bredberg.
//!
//! Each frame every unit of player 1 acts, then every unit of player 2, then
//! cooldowns tick and units regenerate. A unit attacks the closest enemy it can
//! hit, walking straight to it; units are points and ranges are measured between
//! centers. The simulation stops after the given number of frames, when a side is
//! empty or when a frame changed nothing.
//!
//! Differences from the C++ code:
//! - the `unitType` of a unit is replaced by what FAP decides by it: [`Role`],
//!   [`Regen`] and [`Splash`];
//! - [`Unit`] is a struct with the values of the `FAP::Unit` setters; there is
//!   no typestate builder, and the unit types and upgrades are resolved by the
//!   caller;
//! - the simulator owns all its memory from [`FastAPproximation::with_capacity`]
//!   on: adding more units than it holds is an error, and neither adding nor
//!   simulating allocates;
//! - a tank splash kill that moves the tank's target in the vector keeps
//!   tracking the target; C++ keeps an iterator to the slot it left;
//! - the API is renamed: `addIfCombatUnitPlayer1/2` is
//!   [`add_if_combat_unit`](FastAPproximation::add_if_combat_unit) with a
//!   [`Player`], `simulate<tankSplash>` takes [`Options`], `getState` is
//!   [`state`](FastAPproximation::state) and
//!   [`state_mut`](FastAPproximation::state_mut);
//! - `simulate` tells how many frames it ran and why it stopped:
//!   [`SimulationOutcome`].

mod unit;

pub use unit::{DamageType, FAPUnit, Regen, Role, Splash, Unit, UnitSizeType};

use alloc::vec::Vec;
use core::fmt;

use crate::math::sqrt;

/// Frames `simulate` runs in C++ by default: 4 seconds on the fastest speed.
pub const DEFAULT_FRAMES: i32 = 96;

/// One of the two sides. Player one acts first in every frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Player {
    One,
    Two,
}

/// How [`FastAPproximation::simulate`] runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Units with a [`Splash`] (siege tanks) splash: `simulate<true>` in C++.
    pub tank_splash: bool,
}

/// Why [`FastAPproximation::simulate`] stopped, and after how many frames it
/// ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SimulationOutcome {
    /// It ran the frames it was given, whatever the last one left.
    LimitReached { frames: u32 },
    /// A side had no units before the next frame; no frame at all if one had
    /// none at the start.
    SideEliminated { frames: u32 },
    /// Nothing happened in the last frame it ran, which counts.
    Idle { frames: u32 },
}

impl SimulationOutcome {
    /// The frames it ran.
    pub fn frames(self) -> u32 {
        match self {
            SimulationOutcome::LimitReached { frames }
            | SimulationOutcome::SideEliminated { frames }
            | SimulationOutcome::Idle { frames } => frames,
        }
    }
}

/// A player has no room for one more unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacityExceeded;

impl fmt::Display for CapacityExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the simulator holds no more units for this player")
    }
}

impl core::error::Error for CapacityExceeded {}

/// The simulator: `FAP::FastAPproximation`.
#[derive(Clone, Debug)]
pub struct FastAPproximation<T = ()> {
    player1: Vec<FAPUnit<T>>,
    player2: Vec<FAPUnit<T>>,
    /// Slots each player may need: a unit and the marines its bunker turns into.
    reserved: [usize; 2],
    did_something: bool,
}

impl<T: Clone> FastAPproximation<T> {
    /// A simulator for up to `units_per_player` units of each player, counting
    /// every marine a bunker turns into.
    pub fn with_capacity(units_per_player: usize) -> Self {
        FastAPproximation {
            player1: Vec::with_capacity(units_per_player),
            player2: Vec::with_capacity(units_per_player),
            reserved: [0; 2],
            did_something: false,
        }
    }

    /// Adds the unit to `player` if it is a combat unit: it has a weapon or is a
    /// medic, and its role is not [`Role::Ignored`].
    pub fn add_if_combat_unit(
        &mut self,
        player: Player,
        unit: Unit<T>,
    ) -> Result<(), CapacityExceeded> {
        let unit = FAPUnit::from(unit);
        if !is_combat_unit(&unit) {
            return Ok(());
        }
        let (units, player) = match player {
            Player::One => (&mut self.player1, 0),
            Player::Two => (&mut self.player2, 1),
        };
        let slots = match unit.role {
            Role::Bunker { .. } if unit.num_attackers > 0 => unit.num_attackers as usize,
            _ => 1,
        };
        let reserved = self.reserved[player] + slots;
        if reserved > units.capacity() {
            return Err(CapacityExceeded);
        }
        self.reserved[player] = reserved;
        units.push(unit);
        Ok(())
    }

    /// Simulates at most `n_frames` frames ([`DEFAULT_FRAMES`] in C++); a negative
    /// number runs until the fight is over. Can be called again to continue.
    /// C++ returns nothing.
    pub fn simulate(&mut self, n_frames: i32, options: Options) -> SimulationOutcome {
        if options.tank_splash {
            self.run::<true>(n_frames)
        } else {
            self.run::<false>(n_frames)
        }
    }

    /// The units of player one and player two.
    pub fn state(&self) -> (&[FAPUnit<T>], &[FAPUnit<T>]) {
        (&self.player1, &self.player2)
    }

    /// The units of player one and player two, to edit between simulations.
    pub fn state_mut(&mut self) -> (&mut [FAPUnit<T>], &mut [FAPUnit<T>]) {
        (&mut self.player1, &mut self.player2)
    }

    /// Removes all units; the memory stays.
    pub fn clear(&mut self) {
        self.player1.clear();
        self.player2.clear();
        self.reserved = [0; 2];
    }

    fn run<const TANK_SPLASH: bool>(&mut self, mut n_frames: i32) -> SimulationOutcome {
        let mut frames: u32 = 0;
        while n_frames != 0 {
            n_frames = n_frames.wrapping_sub(1);
            if self.player1.is_empty() || self.player2.is_empty() {
                return SimulationOutcome::SideEliminated { frames };
            }
            self.did_something = false;
            self.isimulate::<TANK_SPLASH>();
            frames = frames.wrapping_add(1);
            if !self.did_something && n_frames != 0 {
                return SimulationOutcome::Idle { frames };
            }
        }
        SimulationOutcome::LimitReached { frames }
    }

    fn isimulate<const TANK_SPLASH: bool>(&mut self) {
        let did = &mut self.did_something;
        sim_player::<T, TANK_SPLASH>(&mut self.player1, &mut self.player2, did);
        sim_player::<T, TANK_SPLASH>(&mut self.player2, &mut self.player1, did);

        for fu in self.player1.iter_mut().chain(self.player2.iter_mut()) {
            update_unit(fu);
        }
    }
}

/// `simUnit` of `isimulate` for every unit of one player.
fn sim_player<T: Clone, const TANK_SPLASH: bool>(
    friendly: &mut Vec<FAPUnit<T>>,
    enemy: &mut Vec<FAPUnit<T>>,
    did_something: &mut bool,
) {
    let mut i = 0;
    while i < friendly.len() {
        match friendly[i].role {
            Role::Suicide => {
                if suicide_sim(&mut friendly[i], enemy, did_something) {
                    friendly.remove(i);
                } else {
                    i += 1;
                }
            }
            Role::Medic => {
                medicsim(i, friendly);
                i += 1;
            }
            _ => {
                unitsim::<T, TANK_SPLASH>(&mut friendly[i], enemy, did_something);
                i += 1;
            }
        }
    }
}

/// `updateUnit` of `isimulate`: cooldown, healing flag, regeneration.
fn update_unit<T>(fu: &mut FAPUnit<T>) {
    if fu.attack_cooldown_remaining != 0 {
        fu.attack_cooldown_remaining -= 1;
    }
    fu.did_heal_this_frame = false;

    match fu.regen {
        Regen::ZergHp => {
            if fu.health < fu.max_health {
                fu.health += 4;
            }
            if fu.health > fu.max_health {
                fu.health = fu.max_health;
            }
        }
        Regen::ProtossShields => {
            if fu.shields < fu.max_shields {
                fu.shields += 7;
            }
            if fu.shields > fu.max_shields {
                fu.shields = fu.max_shields;
            }
        }
        Regen::None => {}
    }
}

fn is_combat_unit<T>(u: &FAPUnit<T>) -> bool {
    u.role != Role::Ignored && (u.air_damage != 0 || u.ground_damage != 0 || u.role == Role::Medic)
}

fn deal_damage<T>(fu: &mut FAPUnit<T>, damage: i32, damage_type: DamageType) {
    let mut damage = damage << 8;
    let remaining_shields = fu.shields - damage + (fu.shield_armor << 8);
    if remaining_shields > 0 {
        fu.shields = remaining_shields;
        return;
    } else if fu.shields != 0 {
        damage -= fu.shields + (fu.shield_armor << 8);
        fu.shields = 0;
    }

    if damage == 0 {
        return;
    }

    damage -= fu.armor << 8;

    if damage_type == DamageType::Concussive {
        if fu.unit_size == UnitSizeType::Large {
            damage /= 4;
        } else if fu.unit_size == UnitSizeType::Medium {
            damage /= 2;
        }
    } else if damage_type == DamageType::Explosive {
        if fu.unit_size == UnitSizeType::Small {
            damage /= 2;
        } else if fu.unit_size == UnitSizeType::Medium {
            damage = (damage * 3) / 4;
        }
    }

    fu.health -= damage.max(128);
}

fn dist_squared<T>(u1: &FAPUnit<T>, u2: &FAPUnit<T>) -> i32 {
    let (dx, dy) = (u1.x.wrapping_sub(u2.x), u1.y.wrapping_sub(u2.y));
    dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))
}

/// The enemy closest to `fu` that `fu` has a weapon for, not inside its minimum
/// range; the first one on ties.
fn closest_enemy<T>(fu: &FAPUnit<T>, enemy_units: &[FAPUnit<T>]) -> Option<(usize, i32)> {
    let mut closest: Option<(usize, i32)> = None;
    for (i, enemy) in enemy_units.iter().enumerate() {
        let (damage, min_range_squared) = if enemy.flying {
            (fu.air_damage, fu.air_min_range_squared)
        } else {
            (fu.ground_damage, fu.ground_min_range_squared)
        };
        if damage != 0 {
            let d = dist_squared(fu, enemy);
            if closest.is_none_or(|(_, cd)| d < cd) && d >= min_range_squared {
                closest = Some((i, d));
            }
        }
    }
    closest
}

/// One step of `speed` pixels straight toward `(tx, ty)`.
fn step_toward<T>(fu: &mut FAPUnit<T>, tx: i32, ty: i32) {
    let dx = tx - fu.x;
    let dy = ty - fu.y;
    let length = sqrt(f64::from(dx * dx + dy * dy));
    fu.x += (f64::from(dx) * (f64::from(fu.speed) / length)) as i32;
    fu.y += (f64::from(dy) * (f64::from(fu.speed) / length)) as i32;
}

/// Removes the dead unit at `index` by swapping in the last one.
fn remove_dead<T: Clone>(index: usize, units: &mut Vec<FAPUnit<T>>) {
    let dead = units.swap_remove(index);
    unit_death(dead, units);
}

fn unitsim<T: Clone, const TANK_SPLASH: bool>(
    fu: &mut FAPUnit<T>,
    enemy_units: &mut Vec<FAPUnit<T>>,
    did_something: &mut bool,
) {
    if fu.attack_cooldown_remaining != 0 {
        *did_something = true;
        return;
    }

    if fu.ground_damage == 0 && fu.air_damage == 0 {
        return;
    }

    let Some((mut closest, mut closest_dist_squared)) = closest_enemy(fu, enemy_units) else {
        return;
    };

    if closest_dist_squared as f32 <= fu.speed_squared
        && !(fu.x == enemy_units[closest].x && fu.y == enemy_units[closest].y)
    {
        fu.x = enemy_units[closest].x;
        fu.y = enemy_units[closest].y;
        closest_dist_squared = 0;

        *did_something = true;
    }

    let target_flying = enemy_units[closest].flying;
    let max_range_squared = if target_flying {
        fu.air_max_range_squared
    } else {
        fu.ground_max_range_squared
    };

    if closest_dist_squared <= max_range_squared {
        if target_flying {
            deal_damage(&mut enemy_units[closest], fu.air_damage, fu.air_damage_type);
            fu.attack_cooldown_remaining = fu.air_cooldown;
        } else {
            deal_damage(
                &mut enemy_units[closest],
                fu.ground_damage,
                fu.ground_damage_type,
            );

            if TANK_SPLASH && let Some(splash) = fu.splash {
                closest = tank_splash(fu, splash, closest, enemy_units);
            }

            let target = &enemy_units[closest];
            let higher =
                fu.elevation != -1 && target.elevation != -1 && target.elevation > fu.elevation;
            fu.attack_cooldown_remaining = fu.ground_cooldown << i32::from(higher);
        }

        if enemy_units[closest].health < 1 {
            remove_dead(closest, enemy_units);
        }

        *did_something = true;
    } else if closest_dist_squared as f32 > fu.speed_squared && fu.speed >= 1.0 {
        let (tx, ty) = (enemy_units[closest].x, enemy_units[closest].y);
        step_toward(fu, tx, ty);

        *did_something = true;
    }
}

/// Splash of a siege tank around its target at `closest`, which keeps its
/// health: it is checked by the caller. Returns where the target is now.
fn tank_splash<T: Clone>(
    fu: &FAPUnit<T>,
    splash: Splash,
    mut closest: usize,
    enemy_units: &mut Vec<FAPUnit<T>>,
) -> usize {
    let inner_squared = splash.inner * splash.inner;
    let median_squared = splash.median * splash.median;
    let outer_squared = splash.outer * splash.outer;

    let mut enemy_idx = 0;
    while enemy_idx < enemy_units.len() {
        if enemy_units[enemy_idx].flying || enemy_idx == closest {
            enemy_idx += 1;
            continue;
        }
        // Shell hit point to unit edge.
        let effective_dist_squared =
            dist_squared(&enemy_units[closest], &enemy_units[enemy_idx]) / 4;
        let damage = if effective_dist_squared <= inner_squared {
            Some(fu.ground_damage)
        } else if effective_dist_squared <= median_squared {
            Some(fu.ground_damage / 2)
        } else if effective_dist_squared <= outer_squared {
            Some(fu.ground_damage / 4)
        } else {
            None
        };

        let mut killed = false;
        if let Some(damage) = damage {
            deal_damage(&mut enemy_units[enemy_idx], damage, fu.ground_damage_type);
            if enemy_units[enemy_idx].health < 1 {
                killed = true;
                if closest == enemy_units.len() - 1 {
                    closest = enemy_idx;
                }
                remove_dead(enemy_idx, enemy_units);
            }
        }
        if !killed {
            enemy_idx += 1;
        }
    }
    closest
}

fn medicsim<T>(medic: usize, friendly_units: &mut [FAPUnit<T>]) {
    let fu = &friendly_units[medic];
    let mut closest_healable: Option<(usize, i32)> = None;

    for (i, it) in friendly_units.iter().enumerate() {
        if it.is_organic && it.health < it.max_health && !it.did_heal_this_frame {
            let d = dist_squared(fu, it);
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

/// Returns whether the unit hit and died.
fn suicide_sim<T: Clone>(
    fu: &mut FAPUnit<T>,
    enemy_units: &mut Vec<FAPUnit<T>>,
    did_something: &mut bool,
) -> bool {
    let Some((closest, closest_dist_squared)) = closest_enemy(fu, enemy_units) else {
        return false;
    };

    if closest_dist_squared as f32 <= fu.speed_squared {
        let target = &mut enemy_units[closest];
        if target.flying {
            deal_damage(target, fu.air_damage, fu.air_damage_type);
        } else {
            deal_damage(target, fu.ground_damage, fu.ground_damage_type);
        }

        if target.health < 1 {
            remove_dead(closest, enemy_units);
        }

        *did_something = true;
        true
    } else {
        let (tx, ty) = (enemy_units[closest].x, enemy_units[closest].y);
        step_toward(fu, tx, ty);

        *did_something = true;
        false
    }
}

/// A dead bunker with marines turns into its marines.
fn unit_death<T: Clone>(mut fu: FAPUnit<T>, its_friendlies: &mut Vec<FAPUnit<T>>) {
    let Role::Bunker { marine_max_health } = fu.role else {
        return;
    };
    if fu.num_attackers == 0 {
        return;
    }
    fu.role = Role::Fighter;

    const fn squared_range(tiles: i32) -> i32 {
        (tiles * 32) * (tiles * 32)
    }
    match fu.ground_max_range_squared {
        // No range upgrade.
        r if r == squared_range(5) => fu.ground_max_range_squared = squared_range(4),
        // Range upgrade.
        r if r == squared_range(6) => fu.ground_max_range_squared = squared_range(5),
        _ => {}
    }

    fu.air_max_range_squared = fu.ground_max_range_squared;

    fu.armor = 0;

    fu.max_health = marine_max_health << 8;
    fu.health = fu.max_health;

    fu.ground_cooldown *= fu.num_attackers;
    fu.air_cooldown *= fu.num_attackers;

    for _ in 0..fu.num_attackers - 1 {
        its_friendlies.push(fu.clone());
    }
    its_friendlies.push(fu);
}
